use std::{sync::LazyLock, time::Duration};

use axum::{
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RequestTokenError, Scope, TokenResponse,
    TokenUrl, basic::BasicClient, reqwest,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{
    User,
    backend::{AuthSession, promote_if_listed},
    session_store, start_session,
};
use crate::{
    AppState,
    config::{Config, OauthClientConfig},
    db::{Db, now_unix},
    error::{ApiPath, AppError},
    guest, validate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OauthProvider {
    Google,
    Github,
}

impl OauthProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "google" => Some(Self::Google),
            "github" => Some(Self::Github),
            _ => None,
        }
    }

    /// Authorization URL, token URL, scopes.
    fn spec(self) -> (&'static str, &'static str, &'static [&'static str]) {
        match self {
            Self::Google => (
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                &["openid", "email", "profile"],
            ),
            Self::Github => (
                "https://github.com/login/oauth/authorize",
                "https://github.com/login/oauth/access_token",
                &["read:user", "user:email"],
            ),
        }
    }

    fn client_config(self, config: &Config) -> Option<&OauthClientConfig> {
        match self {
            Self::Google => config.google.as_ref(),
            Self::Github => config.github.as_ref(),
        }
    }
}

pub struct OauthIdentity {
    pub provider: OauthProvider,
    pub provider_user_id: String,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OauthError {
    #[error("provider email not verified")]
    EmailNotVerified,
    #[error("provider email is not a valid address")]
    InvalidEmail,
    #[error("signup is disabled")]
    SignupDisabled,
    #[error("account disabled")]
    AccountDisabled,
    #[error("provider account already linked")]
    AlreadyLinked,
    #[error("verify your email first")]
    VerifyEmailFirst,
    #[error(transparent)]
    App(#[from] AppError),
}

impl OauthError {
    /// `?error=` value for browser redirects.
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmailNotVerified => "email_unverified",
            Self::InvalidEmail => "email_invalid",
            Self::SignupDisabled => "signup_disabled",
            Self::AccountDisabled => "disabled",
            Self::AlreadyLinked => "already_linked",
            Self::VerifyEmailFirst => "verify_email_first",
            Self::App(_) => "internal",
        }
    }
}

impl From<libsql::Error> for OauthError {
    fn from(e: libsql::Error) -> Self {
        Self::App(e.into())
    }
}

impl From<anyhow::Error> for OauthError {
    fn from(e: anyhow::Error) -> Self {
        Self::App(e.into())
    }
}

const LINKED_USER: &str = "SELECT u.id, u.disabled_at IS NOT NULL FROM oauth_accounts o
     JOIN users u ON u.id = o.user_id WHERE o.provider = ?1 AND o.provider_user_id = ?2";

async fn linked_user(
    conn: &libsql::Connection,
    i: &OauthIdentity,
) -> Result<Option<(i64, bool)>, OauthError> {
    let mut rows = conn
        .query(
            LINKED_USER,
            libsql::params![i.provider.as_str(), i.provider_user_id.as_str()],
        )
        .await?;
    Ok(rows
        .next()
        .await?
        .map(|r| Ok::<_, libsql::Error>((r.get(0)?, r.get::<i64>(1)? != 0)))
        .transpose()?)
}

/// Account linking rules; returns the user to sign in.
pub async fn resolve_oauth_user(
    db: &Db,
    config: &Config,
    identity: &OauthIdentity,
) -> Result<User, OauthError> {
    if linked_user(db.conn().await?.reader(), identity)
        .await?
        .is_none()
    {
        match link_or_create(db, config, identity).await {
            // A concurrent first-time callback won the insert; resolve its link on the primary below.
            Err(OauthError::AlreadyLinked) => {}
            r => return r,
        }
    }
    // Decided on the primary (reads inside a transaction go there): the link and the disabled flag may
    // be newer than the replica.
    let conn = db.conn().await?;
    let tx = conn.transaction().await?;
    let linked = linked_user(&tx, identity).await?;
    tx.commit().await?;
    let user_id = match linked {
        None => return Err(OauthError::AlreadyLinked),
        Some((_, true)) => return Err(OauthError::AccountDisabled),
        Some((id, false)) => id,
    };
    promote_if_listed(conn.writer(), config, user_id).await?;
    // Records the writes, so the read below catches the replica up first.
    drop(conn);
    Ok(super::backend::find_user(db, user_id)
        .await?
        .ok_or(AppError::NotFound)?)
}

async fn link_or_create(db: &Db, config: &Config, i: &OauthIdentity) -> Result<User, OauthError> {
    if !i.email_verified {
        return Err(OauthError::EmailNotVerified);
    }
    let email = validate::parse_email(&i.email).map_err(|_| OauthError::InvalidEmail)?;
    let conn = db.conn().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await?;
    let mut rows = tx
        .query(
            "SELECT id, email_verified_at IS NOT NULL, disabled_at IS NOT NULL FROM users WHERE email = ?1",
            [email.as_str()],
        )
        .await?;
    let existing = rows
        .next()
        .await?
        .map(|r| {
            Ok::<_, libsql::Error>((
                r.get::<i64>(0)?,
                r.get::<i64>(1)? != 0,
                r.get::<i64>(2)? != 0,
            ))
        })
        .transpose()?;
    drop(rows);
    let user_id = match existing {
        Some((_, _, true)) => {
            tx.rollback().await?;
            return Err(OauthError::AccountDisabled);
        }
        Some((id, true, false)) => id,
        Some((id, false, false)) => {
            // Claim: the provider proved ownership of the address, so a squatter's OAuth identities, password and
            // sessions go. Links the squatter created stay on the account; the owner can delete them.
            tx.execute(
                "UPDATE users SET password_hash = NULL, email_verified_at = ?2 WHERE id = ?1",
                libsql::params![id, now_unix()],
            )
            .await?;
            session_store::delete_user_sessions(&tx, id).await?;
            tx.execute("DELETE FROM oauth_accounts WHERE user_id = ?1", [id])
                .await?;
            id
        }
        None if !config.signup_enabled => {
            tx.rollback().await?;
            return Err(OauthError::SignupDisabled);
        }
        None => {
            // Unusable provider names are dropped; settings then asks for one.
            let name = Some(validate::normalize_name(i.name.as_deref()))
                .filter(|n| validate::name(n, &()).is_ok());
            let now = now_unix();
            let n = tx
                .execute(
                    "INSERT INTO users (email, email_verified_at, name, created_at) VALUES (?1, ?2, ?3, ?2)
                     ON CONFLICT DO NOTHING",
                    libsql::params![email.as_str(), now, name],
                )
                .await?;
            // A concurrent callback created this email meanwhile.
            if n == 0 {
                tx.rollback().await?;
                return Err(OauthError::AlreadyLinked);
            }
            tx.last_insert_rowid()
        }
    };
    let inserted = tx
        .execute(
            "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT DO NOTHING",
            libsql::params![
                user_id,
                i.provider.as_str(),
                i.provider_user_id.as_str(),
                now_unix()
            ],
        )
        .await?;
    if inserted == 0 {
        tx.rollback().await?;
        return Err(OauthError::AlreadyLinked);
    }
    tx.commit().await?;
    promote_if_listed(conn.writer(), config, user_id).await?;
    // Records the writes, so the read below catches the replica up first.
    drop(conn);
    Ok(super::backend::find_user(db, user_id)
        .await?
        .ok_or(AppError::NotFound)?)
}

/// Connects a provider to an already signed-in user; email is ignored and never changed.
/// The insert only happens while `session_id` still belongs to the user (false = session revoked meanwhile).
pub async fn link_identity(
    db: &Db,
    user_id: i64,
    session_id: &str,
    identity: &OauthIdentity,
) -> Result<bool, OauthError> {
    // The insert decides; on a conflict a read in the same transaction (served by the primary) says
    // which identity holds the slot.
    let conn = db.conn().await?;
    let tx = conn
        .transaction_with_behavior(libsql::TransactionBehavior::Immediate)
        .await?;
    let n = tx
        .execute(
            "INSERT INTO oauth_accounts (user_id, provider, provider_user_id, created_at)
             SELECT ?1, ?2, ?3, ?4
             WHERE EXISTS (SELECT 1 FROM sessions WHERE id = ?5 AND user_id = ?1)
             ON CONFLICT DO NOTHING",
            libsql::params![
                user_id,
                identity.provider.as_str(),
                identity.provider_user_id.as_str(),
                now_unix(),
                session_id
            ],
        )
        .await?;
    let linked = if n == 1 {
        Ok(true)
    } else {
        // Nothing inserted: a conflicting identity, or the session was revoked meanwhile.
        let mut rows = tx
            .query(
                "SELECT user_id = ?1 AND provider_user_id = ?3 FROM oauth_accounts
                 WHERE provider = ?2 AND (provider_user_id = ?3 OR user_id = ?1)",
                libsql::params![
                    user_id,
                    identity.provider.as_str(),
                    identity.provider_user_id.as_str()
                ],
            )
            .await?;
        match rows.next().await? {
            Some(r) if r.get::<i64>(0)? != 0 => Ok(true),
            Some(_) => Err(OauthError::AlreadyLinked),
            None => Ok(false),
        }
    };
    tx.commit().await?;
    linked
}

pub async fn list_providers(db: &Db, user_id: i64) -> Result<Vec<String>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(
            "SELECT provider FROM oauth_accounts WHERE user_id = ?1 ORDER BY provider",
            [user_id],
        )
        .await?;
    let mut out = vec![];
    while let Some(r) = rows.next().await? {
        out.push(r.get(0)?);
    }
    Ok(out)
}

/// Refuses to remove the last sign-in method (a password only counts while password auth is on).
pub async fn unlink(
    db: &Db,
    config: &Config,
    user_id: i64,
    provider: OauthProvider,
) -> Result<(), AppError> {
    let n = db
        .conn()
        .await?
        .execute(
            "DELETE FROM oauth_accounts WHERE user_id = ?1 AND provider = ?2 AND (
                 (?3 AND EXISTS (SELECT 1 FROM users WHERE id = ?1 AND password_hash IS NOT NULL))
                 OR EXISTS (SELECT 1 FROM oauth_accounts WHERE user_id = ?1 AND provider != ?2))",
            libsql::params![user_id, provider.as_str(), config.password_auth_enabled],
        )
        .await?;
    if n > 0 {
        return Ok(());
    }
    if list_providers(db, user_id)
        .await?
        .iter()
        .any(|p| p == provider.as_str())
    {
        Err(AppError::Conflict(
            "cannot remove the last sign-in method".into(),
        ))
    } else {
        Err(AppError::NotFound)
    }
}

type Client = BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn client(config: &Config, provider: OauthProvider) -> Option<Client> {
    let c = provider.client_config(config)?;
    let (auth, token, _) = provider.spec();
    let redirect = config
        .base_url
        .as_ref()?
        .join(&format!("api/v1/auth/oauth/{}/callback", provider.as_str()))
        .ok()?;
    Some(
        BasicClient::new(ClientId::new(c.client_id.clone()))
            .set_client_secret(ClientSecret::new(c.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(auth.into()).ok()?)
            .set_token_uri(TokenUrl::new(token.into()).ok()?)
            .set_redirect_uri(RedirectUrl::from_url(redirect)),
    )
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        // Following redirects with a bearer token invites SSRF/token leaks.
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent("tiiny")
        .build()
        .expect("http client")
});

const SESSION_KEY: &str = "tiiny.oauth";

#[derive(Serialize, Deserialize)]
struct Pending {
    provider: OauthProvider,
    state: String,
    verifier: String,
    link_user_id: Option<i64>,
    next: Option<String>,
}

/// Same-origin path for `?next=`, mirroring web/src/lib/safe-next.ts; anything else is dropped.
fn safe_next(v: Option<String>) -> Option<String> {
    v.filter(|n| {
        n.len() <= 2048
            && n.starts_with('/')
            && !n.starts_with("//")
            && !n.starts_with("/\\")
            && n.bytes().all(|b| b.is_ascii_graphic())
    })
}

fn found(location: &str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, location.to_owned())]).into_response()
}

#[derive(Deserialize)]
pub struct StartQuery {
    link: Option<String>,
    next: Option<String>,
}

pub async fn start(
    State(s): State<AppState>,
    ApiPath(provider): ApiPath<String>,
    q: Result<Query<StartQuery>, QueryRejection>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let provider = OauthProvider::parse(&provider).ok_or(AppError::NotFound)?;
    let client = client(&s.config, provider).ok_or(AppError::NotFound)?;
    let Ok(Query(q)) = q else {
        return Ok(found("/login?error=state"));
    };
    let next = safe_next(q.next);
    let link_user_id = match (q.link.as_deref() == Some("1"), &auth.user) {
        (true, None) => return Ok(found("/login?error=unauthorized")),
        (true, Some(u)) if !u.email_verified => {
            return Ok(link_error(OauthError::VerifyEmailFirst));
        }
        (true, Some(u)) => Some(u.id),
        // A forced cross-site GET must not switch a signed-in browser to another account.
        (false, Some(_)) => return Ok(found(next.as_deref().unwrap_or("/dashboard"))),
        (false, None) => None,
    };
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (_, _, scopes) = provider.spec();
    let (url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scopes(scopes.iter().map(|s| Scope::new((*s).into())))
        .set_pkce_challenge(challenge)
        .url();
    let pending = Pending {
        provider,
        state: state.into_secret(),
        verifier: verifier.into_secret(),
        link_user_id,
        next,
    };
    auth.session
        .insert(SESSION_KEY, pending)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(found(url.as_str()))
}

#[derive(Deserialize, garde::Validate)]
pub struct CallbackQuery {
    #[garde(inner(length(bytes, max = 2048)))]
    code: Option<String>,
    #[garde(inner(length(bytes, max = 256)))]
    state: Option<String>,
    #[garde(skip)]
    error: Option<String>,
}

fn redirect_error(base: &str, e: OauthError) -> Response {
    if let OauthError::App(e) = &e {
        tracing::error!(error = ?e, "oauth callback failed");
    }
    found(&format!("{base}?error={}", e.code()))
}

fn link_error(e: OauthError) -> Response {
    redirect_error("/dashboard/settings", e)
}

pub async fn callback(
    State(s): State<AppState>,
    ApiPath(provider): ApiPath<String>,
    q: Result<Query<CallbackQuery>, QueryRejection>,
    mut auth: AuthSession,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let provider = OauthProvider::parse(&provider).ok_or(AppError::NotFound)?;
    let client = client(&s.config, provider).ok_or(AppError::NotFound)?;
    // Removed before any check so a state value is never accepted twice.
    let pending = auth
        .session
        .remove::<Pending>(SESSION_KEY)
        .await
        .ok()
        .flatten();
    let Some(q) = q
        .ok()
        .and_then(|Query(q)| garde::Unvalidated::new(q).validate().ok())
        .map(garde::Valid::into_inner)
    else {
        return Ok(found("/login?error=state"));
    };
    let state_ok =
        |p: &Pending| p.provider == provider && q.state.as_deref() == Some(p.state.as_str());
    // User cancelled at the provider; a matching link-mode flow falls through to report on settings.
    if q.error.is_some() && !pending.as_ref().is_some_and(state_ok) {
        return Ok(found("/login?error=denied"));
    }
    let Some(pending) = pending.filter(state_ok) else {
        return Ok(found("/login?error=state"));
    };
    let fail_to = if pending.link_user_id.is_some() {
        "/dashboard/settings"
    } else {
        "/login"
    };
    if let Some(uid) = pending.link_user_id {
        match &auth.user {
            Some(u) if u.id == uid && u.email_verified => {}
            Some(u) if u.id == uid => return Ok(link_error(OauthError::VerifyEmailFirst)),
            _ => return Ok(found("/login?error=state")),
        }
    }
    let Some(code) = q.code.filter(|_| q.error.is_none()) else {
        return Ok(found(&format!("{fail_to}?error=denied")));
    };
    let identity = match fetch_identity(&client, provider, code, pending.verifier).await {
        Ok(i) => i,
        Err(why) => {
            tracing::warn!(provider = provider.as_str(), why, "oauth exchange failed");
            return Ok(found(&format!("{fail_to}?error=provider")));
        }
    };
    if let Some(uid) = pending.link_user_id {
        let sid = auth.session.id().map(|i| i.to_string()).unwrap_or_default();
        return Ok(match link_identity(&s.db, uid, &sid, &identity).await {
            Ok(true) => {
                tracing::info!(
                    user_id = uid,
                    provider = provider.as_str(),
                    "sign-in provider connected"
                );
                found("/dashboard/settings")
            }
            Ok(false) => found("/login?error=state"),
            Err(e) => {
                tracing::debug!(
                    user_id = uid,
                    provider = provider.as_str(),
                    error = e.code(),
                    "provider connect refused"
                );
                link_error(e)
            }
        });
    }
    match resolve_oauth_user(&s.db, &s.config, &identity).await {
        Ok(user) => {
            start_session(&mut auth, &user).await?;
            tracing::info!(user_id = user.id, method = provider.as_str(), "signed in");
            let out = guest::claim_headers(&s.db, &s.config, user.id, &headers).await?;
            Ok((out, found(pending.next.as_deref().unwrap_or("/dashboard"))).into_response())
        }
        Err(e) => {
            tracing::debug!(
                provider = provider.as_str(),
                error = e.code(),
                "oauth sign-in refused"
            );
            Ok(redirect_error(fail_to, e))
        }
    }
}

/// Errors are static labels so provider responses (which may carry tokens) never reach logs.
async fn fetch_identity(
    client: &Client,
    provider: OauthProvider,
    code: String,
    verifier: String,
) -> Result<OauthIdentity, &'static str> {
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(verifier))
        .request_async(&*HTTP)
        .await
        .map_err(|e| match e {
            RequestTokenError::ServerResponse(_) => "token endpoint rejected code",
            RequestTokenError::Request(_) => "token request failed",
            RequestTokenError::Parse(..) => "token response unparseable",
            RequestTokenError::Other(_) => "token exchange failed",
        })?;
    let bearer = token.access_token().secret();
    match provider {
        OauthProvider::Google => {
            #[derive(Deserialize)]
            struct Info {
                sub: String,
                email: Option<String>,
                #[serde(default)]
                email_verified: bool,
                name: Option<String>,
            }
            let i: Info =
                get_json("https://openidconnect.googleapis.com/v1/userinfo", bearer).await?;
            Ok(OauthIdentity {
                provider,
                provider_user_id: i.sub,
                email_verified: i.email_verified && i.email.is_some(),
                email: i.email.unwrap_or_default().to_lowercase(),
                name: i.name,
            })
        }
        OauthProvider::Github => {
            #[derive(Deserialize)]
            struct GhUser {
                id: i64,
                name: Option<String>,
            }
            #[derive(Deserialize)]
            struct GhEmail {
                email: String,
                primary: bool,
                verified: bool,
            }
            let (u, emails): (GhUser, Vec<GhEmail>) = tokio::try_join!(
                get_json("https://api.github.com/user", bearer),
                get_json("https://api.github.com/user/emails", bearer)
            )?;
            let primary = emails.into_iter().find(|e| e.primary);
            Ok(OauthIdentity {
                provider,
                provider_user_id: u.id.to_string(),
                email_verified: primary.as_ref().is_some_and(|e| e.verified),
                email: primary.map(|e| e.email.to_lowercase()).unwrap_or_default(),
                name: u.name,
            })
        }
    }
}

async fn get_json<T: DeserializeOwned>(url: &str, bearer: &str) -> Result<T, &'static str> {
    let res = HTTP
        .get(url)
        .bearer_auth(bearer)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| "userinfo request failed")?;
    if !res.status().is_success() {
        return Err("userinfo request rejected");
    }
    let body = res.bytes().await.map_err(|_| "userinfo read failed")?;
    serde_json::from_slice(&body).map_err(|_| "userinfo unparseable")
}
