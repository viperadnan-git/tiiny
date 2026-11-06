pub mod backend;
pub mod email;
pub mod oauth;
pub mod password;
pub mod session_store;

use std::{net::IpAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{FromRequestParts, Request, State},
    http::{HeaderMap, Method, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder, key_extractor::KeyExtractor,
};
use tower_sessions::{Expiry, SessionManagerLayer, cookie::SameSite};

use crate::{
    AppState, ClientAddr,
    db::{Db, now_unix},
    error::{ApiJson, ApiPath, AppError},
    guest,
    links::model::UserRef,
    validate,
};
use backend::{AuthBackend, AuthSession, PasswordCredentials, find_user, promote_if_listed};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    /// The `users.role` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
        }
    }
}

#[derive(Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub role: Role,
    pub disabled: bool,
    pub has_password: bool,
    pub created_at: i64,
    #[serde(skip)]
    auth_hash: Vec<u8>,
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    pub fn as_user_ref(&self) -> UserRef {
        UserRef {
            id: self.id,
            is_admin: self.is_admin(),
        }
    }
}

/// Session key of axum-login's login data (`user_id`, `auth_hash`).
pub(crate) const LOGIN_DATA_KEY: &str = "axum-login.data";

pub struct CurrentUser(pub User);

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let session = AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|(_, msg)| AppError::Internal(anyhow::anyhow!(msg)))?;
        session.user.map(CurrentUser).ok_or(AppError::Unauthorized)
    }
}

pub struct AdminUser(pub User);

impl<S: Send + Sync> FromRequestParts<S> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        if user.is_admin() {
            Ok(AdminUser(user))
        } else {
            Err(AppError::Forbidden("only admins can do this".into()))
        }
    }
}

fn login_error(e: axum_login::Error<AuthBackend>) -> AppError {
    match e {
        axum_login::Error::Backend(e) => e,
        e => AppError::Internal(e.into()),
    }
}

/// Wraps `/api/v1`: session cookie + axum-login. Kept off the redirect path so it never touches the DB.
pub fn auth_layer(
    state: &AppState,
) -> axum_login::AuthManagerLayer<AuthBackend, session_store::LibsqlSessionStore> {
    let sessions =
        SessionManagerLayer::new(session_store::LibsqlSessionStore::new(state.db.clone()))
            .with_name("tiiny_session")
            .with_http_only(true)
            .with_same_site(SameSite::Lax)
            .with_secure(state.config.https())
            .with_expiry(Expiry::OnInactivity(
                tower_sessions::cookie::time::Duration::days(30),
            ))
            // The store drops writes that only extend expiry by < 1 day.
            .with_always_save(true);
    axum_login::AuthManagerLayerBuilder::new(
        AuthBackend::new(state.db.clone(), state.config.clone()),
        sessions,
    )
    .with_data_key(LOGIN_DATA_KEY)
    .build()
}

/// Also on bodiless requests: cross-site forms can't send `application/json` without a CORS preflight (CSRF guard).
pub async fn require_json_for_mutations(req: Request, next: Next) -> Response {
    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let is_json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"));
    if mutating && !is_json {
        return AppError::UnsupportedMediaType.into_response();
    }
    next.run(req).await
}

/// Keys on the [`ClientAddr`] that [`client_addr`] stored; `None` (no resolvable IP) is one shared bucket.
#[derive(Clone)]
struct RateLimitKey;

impl KeyExtractor for RateLimitKey {
    type Key = Option<IpAddr>;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let ClientAddr(ip) = req
            .extensions()
            .get()
            .copied()
            .ok_or(GovernorError::UnableToExtractKey)?;
        Ok(ip.map(rate_limit_key))
    }
}

/// `KeyExtractor` only sees `&Request`, so the extractor runs here first.
async fn client_addr(addr: ClientAddr, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(addr);
    next.run(req).await
}

/// IPv6 clients usually control a whole /64, so they share one bucket.
pub(crate) fn rate_limit_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.into(),
            None => std::net::Ipv6Addr::from_bits(v6.to_bits() & !(u64::MAX as u128)).into(),
        },
        v4 => v4,
    }
}

pub fn router() -> Router<AppState> {
    let config = GovernorConfigBuilder::default()
        .period(guest::LIMIT_PERIOD)
        .burst_size(guest::LIMIT_BURST)
        .key_extractor(RateLimitKey)
        .finish()
        .expect("non-zero rate limit");
    let limiter = Arc::downgrade(config.limiter());
    guest::prune_every_minute(move || limiter.upgrade().map(|l| l.retain_recent()).is_some());
    let governor = GovernorLayer::new(config).error_handler(|e| match e {
        GovernorError::TooManyRequests { wait_time, .. } => AppError::RateLimited {
            retry_after: wait_time,
        }
        .into_response(),
        e => AppError::Internal(anyhow::anyhow!(e.to_string())).into_response(),
    });
    Router::new()
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/resend-verification", post(resend_verification))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/password", put(change_password))
        .route("/oauth/{provider}/start", get(oauth::start))
        .route("/oauth/{provider}/callback", get(oauth::callback))
        .layer(governor)
        .layer(axum::middleware::from_fn(client_addr))
        .route("/verify-email", post(verify_email))
        .route("/providers", get(providers))
        .route("/logout", post(logout))
        .route("/me", get(me).patch(update_me).delete(delete_me))
        .route("/accounts", get(accounts))
        .route("/accounts/{provider}", delete(unlink_account))
}

async fn providers(State(s): State<AppState>) -> Json<serde_json::Value> {
    let c = &s.config;
    Json(json!({
        "password": c.password_auth_enabled,
        "google": c.google.is_some(),
        "github": c.github.is_some(),
        "email_verification": s.mailer.is_enabled(),
        "signup": c.signup_enabled,
        "guest_links": c.guest_links(),
    }))
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct SignupBody {
    #[serde(deserialize_with = "validate::de_email")]
    #[garde(custom(validate::email))]
    email: String,
    #[garde(custom(validate::password))]
    password: String,
    #[serde(default, deserialize_with = "validate::de_name")]
    #[garde(custom(validate::name))]
    name: String,
}

async fn start_session(auth: &mut AuthSession, user: &User) -> Result<(), AppError> {
    // axum-login's `login` rotates the session id only when the login data has no auth hash; rotate here
    // when it has one, so the id rotates exactly once either way.
    let login: Option<serde_json::Value> = auth
        .session
        .get(LOGIN_DATA_KEY)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if login.is_some_and(|d| !d["auth_hash"].is_null()) {
        auth.session
            .cycle_id()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    auth.login(user).await.map_err(login_error)
}

async fn signup(
    State(s): State<AppState>,
    _: PasswordAuth,
    _: SignupEnabled,
    mut auth: AuthSession,
    headers: HeaderMap,
    ApiJson(body): ApiJson<SignupBody>,
) -> Result<(StatusCode, HeaderMap, Json<User>), AppError> {
    let SignupBody {
        email,
        password,
        name,
    } = body.into_inner();
    let hash = password::hash(password).await?;
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "INSERT INTO users (email, password_hash, name, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT DO NOTHING RETURNING {}",
                backend::COLS
            ),
            libsql::params![email, hash, name, now_unix()],
        )
        .await?;
    // A live row keeps the INSERT's statement (and its write lock) open.
    let mut user = match rows.next().await? {
        Some(r) => backend::from_row(&r)?.0,
        None => {
            return Err(AppError::Conflict(
                "email is already registered. Sign in instead".into(),
            ));
        }
    };
    drop(rows);
    if backend::promote_listed(s.db.conn().await?.writer(), &s.config, Some(user.id)).await? > 0 {
        user.role = Role::Admin;
    }
    if s.mailer.is_enabled() {
        let (s, user) = (s.clone(), user.clone());
        tokio::spawn(async move {
            if let Err(e) =
                email::send_verification(&s.mailer, &s.db, &s.config, user.id, &user.email).await
            {
                tracing::warn!(error = %e, user_id = user.id, "verification email failed");
            }
        });
    }
    start_session(&mut auth, &user).await?;
    tracing::info!(user_id = user.id, admin = user.is_admin(), "signed up");
    let out = guest::claim_headers(&s.db, &s.config, user.id, &headers).await?;
    Ok((StatusCode::CREATED, out, Json(user)))
}

async fn login(
    State(s): State<AppState>,
    _: PasswordAuth,
    mut auth: AuthSession,
    headers: HeaderMap,
    ApiJson(creds): ApiJson<PasswordCredentials>,
) -> Result<(HeaderMap, Json<User>), AppError> {
    let user = auth
        .authenticate(creds.into_inner())
        .await
        .map_err(login_error)?
        .ok_or_else(|| {
            tracing::debug!("sign-in rejected: wrong email or password");
            AppError::InvalidCredentials
        })?;
    start_session(&mut auth, &user).await?;
    tracing::info!(user_id = user.id, method = "password", "signed in");
    let out = guest::claim_headers(&s.db, &s.config, user.id, &headers).await?;
    Ok((out, Json(user)))
}

async fn logout(mut auth: AuthSession) -> Result<StatusCode, AppError> {
    if let Some(user) = auth.logout().await.map_err(login_error)? {
        tracing::debug!(user_id = user.id, "signed out");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn me(CurrentUser(user): CurrentUser) -> Json<User> {
    Json(user)
}

// Route gates. Extractors run in argument order, so placed before `ApiJson` they answer before the body is validated.

/// 404 unless `TIINY_PASSWORD_AUTH_ENABLED`.
pub struct PasswordAuth;

impl FromRequestParts<AppState> for PasswordAuth {
    type Rejection = AppError;

    async fn from_request_parts(_: &mut Parts, s: &AppState) -> Result<Self, AppError> {
        s.config
            .password_auth_enabled
            .then_some(Self)
            .ok_or(AppError::NotFound)
    }
}

/// 403 unless `TIINY_SIGNUP_ENABLED`.
pub struct SignupEnabled;

impl FromRequestParts<AppState> for SignupEnabled {
    type Rejection = AppError;

    async fn from_request_parts(_: &mut Parts, s: &AppState) -> Result<Self, AppError> {
        s.config
            .signup_enabled
            .then_some(Self)
            .ok_or_else(|| AppError::Forbidden("this server isn't accepting new accounts".into()))
    }
}

/// 404 unless SMTP is configured.
pub struct MailEnabled;

impl FromRequestParts<AppState> for MailEnabled {
    type Rejection = AppError;

    async fn from_request_parts(_: &mut Parts, s: &AppState) -> Result<Self, AppError> {
        s.mailer
            .is_enabled()
            .then_some(Self)
            .ok_or(AppError::NotFound)
    }
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
struct TokenBody {
    token: String,
}

async fn verify_email(
    State(s): State<AppState>,
    _: MailEnabled,
    mut auth: AuthSession,
    headers: HeaderMap,
    ApiJson(body): ApiJson<TokenBody>,
) -> Result<(StatusCode, HeaderMap), AppError> {
    validate::token(&body.token)?;
    // Only someone signed in to the account (i.e. who knows its password) may verify it.
    let user = auth.user.clone().ok_or(AppError::Unauthorized)?;
    // Leaves the token and other sessions alone: there's nothing to confirm.
    if user.email_verified {
        return Err(AppError::Conflict("email already verified".into()));
    }
    let conn = s.db.conn().await?;
    let tx = conn.transaction().await?;
    let user_id = email::consume_token(
        &tx,
        &body.token,
        email::TokenPurpose::VerifyEmail,
        Some(user.id),
    )
    .await?;
    tx.execute(
        "UPDATE users SET email_verified_at = COALESCE(email_verified_at, ?2) WHERE id = ?1",
        libsql::params![user_id, now_unix()],
    )
    .await?;
    // All of them: this one is cycled by `start_session` below.
    session_store::delete_user_sessions(&tx, user_id).await?;
    promote_if_listed(&tx, &s.config, user_id).await?;
    tx.commit().await?;
    // Records the writes, so the read below catches the replica up first.
    drop(conn);
    let user = find_user(&s.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    start_session(&mut auth, &user).await?;
    tracing::info!(user_id = user.id, "email verified");
    let out = guest::claim_headers(&s.db, &s.config, user.id, &headers).await?;
    Ok((StatusCode::NO_CONTENT, out))
}

async fn resend_verification(
    State(s): State<AppState>,
    _: MailEnabled,
    CurrentUser(user): CurrentUser,
) -> Result<StatusCode, AppError> {
    if user.email_verified {
        return Err(AppError::Conflict("email already verified".into()));
    }
    email::send_verification(&s.mailer, &s.db, &s.config, user.id, &user.email).await?;
    tracing::debug!(user_id = user.id, "verification email resent");
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct ForgotBody {
    #[serde(deserialize_with = "validate::de_email")]
    #[garde(custom(validate::email))]
    email: String,
}

async fn forgot_password(
    State(s): State<AppState>,
    _: MailEnabled,
    _: PasswordAuth,
    ApiJson(body): ApiJson<ForgotBody>,
) -> Result<StatusCode, AppError> {
    let found = backend::find_by_email(&s.db, &body.email).await?;
    tracing::debug!(
        sent = found.as_ref().is_some_and(|u| !u.disabled),
        "password reset requested"
    );
    if let Some(user) = found.filter(|u| !u.disabled) {
        // Sent off-request so response time doesn't reveal whether the account exists.
        tokio::spawn(async move {
            if let Err(e) =
                email::send_reset(&s.mailer, &s.db, &s.config, user.id, &user.email).await
            {
                tracing::warn!(error = %e, user_id = user.id, "reset email failed");
            }
        });
    }
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct ResetBody {
    #[garde(skip)]
    token: String,
    #[garde(custom(validate::password))]
    password: String,
}

async fn reset_password(
    State(s): State<AppState>,
    _: MailEnabled,
    _: PasswordAuth,
    ApiJson(body): ApiJson<ResetBody>,
) -> Result<StatusCode, AppError> {
    let body = body.into_inner();
    validate::token(&body.token)?;
    let hash = password::hash(body.password).await?;
    let conn = s.db.conn().await?;
    let tx = conn.transaction().await?;
    let user_id =
        email::consume_token(&tx, &body.token, email::TokenPurpose::ResetPassword, None).await?;
    recover_account(&tx, user_id, &hash, true).await?;
    promote_if_listed(&tx, &s.config, user_id).await?;
    tx.commit().await?;
    tracing::info!(user_id, "password reset by email");
    Ok(StatusCode::NO_CONTENT)
}

async fn accounts(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let list = oauth::list_providers(&s.db, user.id).await?;
    Ok(Json(
        list.into_iter().map(|p| json!({ "provider": p })).collect(),
    ))
}

async fn unlink_account(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiPath(provider): ApiPath<String>,
) -> Result<StatusCode, AppError> {
    let provider = oauth::OauthProvider::parse(&provider).ok_or(AppError::NotFound)?;
    oauth::unlink(&s.db, &s.config, user.id, provider).await?;
    tracing::info!(
        user_id = user.id,
        provider = provider.as_str(),
        "sign-in provider disconnected"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct PasswordBody {
    /// Checked against the stored hash, not by rule.
    #[garde(skip)]
    current_password: Option<String>,
    #[garde(custom(validate::password))]
    new_password: String,
}

async fn change_password(
    State(s): State<AppState>,
    _: PasswordAuth,
    CurrentUser(user): CurrentUser,
    mut auth: AuthSession,
    headers: HeaderMap,
    ApiJson(body): ApiJson<PasswordBody>,
) -> Result<(StatusCode, HeaderMap), AppError> {
    let body = body.into_inner();
    // The UPDATE below only applies while the hash is still this one.
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query("SELECT password_hash FROM users WHERE id = ?1", [user.id])
        .await?;
    let current_hash: Option<String> = rows.next().await?.ok_or(AppError::Unauthorized)?.get(0)?;
    drop(rows);
    if let Some(hash) = current_hash.clone() {
        let current = body
            .current_password
            .ok_or_else(|| AppError::Unprocessable("current password is required".into()))?;
        if !password::verify(current, hash).await? {
            return Err(AppError::Forbidden("current password is incorrect".into()));
        }
    }
    let first = current_hash.is_none();
    let hash = password::hash(body.new_password).await?;
    let mut rows = conn
        .query(
            &format!(
                "UPDATE users SET password_hash = ?2 WHERE id = ?1 AND password_hash IS ?3
                 RETURNING {}",
                backend::COLS
            ),
            libsql::params![user.id, hash, current_hash],
        )
        .await?;
    let user = match rows.next().await? {
        Some(r) => backend::from_row(&r)?.0,
        None => {
            return Err(AppError::Conflict(
                "the password was changed meanwhile; try again".into(),
            ));
        }
    };
    drop(rows);
    session_store::delete_user_sessions(conn.writer(), user.id).await?;
    // The session auth hash derives from password_hash, so re-login under a fresh id.
    start_session(&mut auth, &user).await?;
    tracing::info!(user_id = user.id, first, "password changed");
    let out = guest::claim_headers(&s.db, &s.config, user.id, &headers).await?;
    Ok((StatusCode::NO_CONTENT, out))
}

#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
struct UpdateMeBody {
    #[serde(default, deserialize_with = "validate::de_name")]
    #[garde(custom(validate::name))]
    name: String,
}

async fn update_me(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    ApiJson(body): ApiJson<UpdateMeBody>,
) -> Result<Json<User>, AppError> {
    let conn = s.db.conn().await?;
    let mut rows = conn
        .query(
            &format!(
                "UPDATE users SET name = ?2 WHERE id = ?1 RETURNING {}",
                backend::COLS
            ),
            libsql::params![user.id, body.into_inner().name],
        )
        .await?;
    let row = rows.next().await?.ok_or(AppError::Unauthorized)?;
    let user = backend::from_row(&row)?.0;
    // Autocommit finishes when the statement is drained; surface its error here.
    while rows.next().await?.is_some() {}
    drop(rows);
    Ok(Json(user))
}

async fn delete_me(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    mut auth: AuthSession,
) -> Result<StatusCode, AppError> {
    let slugs = delete_user(&s.db, user.id).await?;
    for slug in &slugs {
        s.cache.evict(slug);
    }
    tracing::info!(user_id = user.id, links = slugs.len(), "account deleted");
    auth.logout().await.map_err(login_error)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Within the caller's transaction: sets a new password and drops every linked identity and session (account recovery).
pub async fn recover_account(
    tx: &libsql::Connection,
    user_id: i64,
    password_hash: &str,
    mark_verified: bool,
) -> libsql::Result<()> {
    tx.execute(
        "UPDATE users SET password_hash = ?2,
             email_verified_at = CASE WHEN ?4 THEN COALESCE(email_verified_at, ?3) ELSE email_verified_at END
         WHERE id = ?1",
        libsql::params![user_id, password_hash, now_unix(), mark_verified],
    )
    .await?;
    // Drop any identity linked by a prior holder.
    tx.execute("DELETE FROM oauth_accounts WHERE user_id = ?1", [user_id])
        .await?;
    session_store::delete_user_sessions(tx, user_id).await
}

/// Retires permanent slugs, then deletes the user (cascading links) and their sessions. Returns the user's slugs.
pub async fn delete_user(db: &Db, user_id: i64) -> Result<Vec<String>, AppError> {
    let conn = db.conn().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await?;
    let mut rows = tx
        .query("SELECT slug FROM links WHERE user_id = ?1", [user_id])
        .await?;
    let mut slugs = vec![];
    while let Some(r) = rows.next().await? {
        slugs.push(r.get::<String>(0)?);
    }
    drop(rows);
    tx.execute(
        "INSERT OR IGNORE INTO retired_slugs (slug, retired_at)
         SELECT slug, ?2 FROM links
         WHERE user_id = ?1 AND (permanent_at IS NOT NULL OR blocked_at IS NOT NULL)",
        libsql::params![user_id, now_unix()],
    )
    .await?;
    let n = tx
        .execute(
            &format!(
                "DELETE FROM users WHERE id = ?1 AND (role != 'admin' OR {})",
                backend::OTHER_ACTIVE_ADMIN
            ),
            [user_id],
        )
        .await?;
    if n == 0 {
        let exists = tx
            .query("SELECT 1 FROM users WHERE id = ?1", [user_id])
            .await?
            .next()
            .await?
            .is_some();
        tx.rollback().await?;
        return Err(if exists {
            AppError::Conflict(
                "can't delete the last admin. Make another user an admin first".into(),
            )
        } else {
            AppError::NotFound
        });
    }
    // sessions and link_quota have no FK to users, so they don't cascade. A reused id would inherit the quota.
    session_store::delete_user_sessions(&tx, user_id).await?;
    tx.execute("DELETE FROM link_quota WHERE owner = 'u:' || ?1", [user_id])
        .await?;
    tx.commit().await?;
    Ok(slugs)
}

#[cfg(test)]
mod tests {
    use super::rate_limit_key;
    use std::net::IpAddr;

    #[test]
    fn rate_limit_key_masks_ipv6_to_64() {
        let k = |s: &str| rate_limit_key(s.parse().unwrap());
        assert_eq!(k("2001:db8:1:2:aaaa::1"), k("2001:db8:1:2:ffff::9"));
        assert_eq!(
            k("2001:db8:1:2:aaaa::1"),
            "2001:db8:1:2::".parse::<IpAddr>().unwrap()
        );
        assert_ne!(k("2001:db8:1:2::1"), k("2001:db8:1:3::1"));
        assert_eq!(k("1.2.3.4"), "1.2.3.4".parse::<IpAddr>().unwrap());
        assert_eq!(k("::ffff:1.2.3.4"), "1.2.3.4".parse::<IpAddr>().unwrap());
    }
}
