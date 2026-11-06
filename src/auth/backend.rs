use std::sync::Arc;

use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::Deserialize;

use super::{
    Role, User,
    password::{self, DUMMY_HASH},
};
use crate::{config::Config, db::Db, error::AppError};

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Clone)]
pub struct AuthBackend {
    db: Db,
    config: Arc<Config>,
}

/// No rules: any failure is the same generic 401 as a wrong password.
#[derive(Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PasswordCredentials {
    #[serde(deserialize_with = "crate::validate::de_email")]
    pub email: String,
    pub password: String,
}

/// Holds when user `?1` is not the only active admin.
pub(crate) const OTHER_ACTIVE_ADMIN: &str =
    "EXISTS (SELECT 1 FROM users WHERE role = 'admin' AND id != ?1 AND disabled_at IS NULL)";

pub(crate) const COLS: &str = "id, email, email_verified_at IS NOT NULL, name, role, disabled_at IS NOT NULL, password_hash, created_at";

pub(crate) fn from_row(r: &libsql::Row) -> Result<(User, Option<String>), AppError> {
    let password_hash: Option<String> = r.get(6)?;
    Ok((
        User {
            id: r.get(0)?,
            email: r.get(1)?,
            email_verified: r.get::<i64>(2)? != 0,
            name: r.get(3)?,
            role: if r.get::<String>(4)? == Role::Admin.as_str() {
                Role::Admin
            } else {
                Role::User
            },
            disabled: r.get::<i64>(5)? != 0,
            has_password: password_hash.is_some(),
            created_at: r.get(7)?,
            auth_hash: auth_hash(password_hash.as_deref()),
        },
        password_hash,
    ))
}

/// Kept in the session payload; a digest so the PHC string never leaves `users`.
fn auth_hash(password_hash: Option<&str>) -> Vec<u8> {
    super::email::hash_token(password_hash.unwrap_or("oauth")).into_bytes()
}

async fn find_where(
    db: &Db,
    clause: &str,
    param: libsql::Value,
) -> Result<Option<(User, Option<String>)>, AppError> {
    let conn = db.conn().await?;
    let mut rows = conn
        .query(&format!("SELECT {COLS} FROM users WHERE {clause}"), [param])
        .await?;
    rows.next().await?.as_ref().map(from_row).transpose()
}

pub async fn find_user(db: &Db, id: i64) -> Result<Option<User>, AppError> {
    Ok(find_where(db, "id = ?1", id.into()).await?.map(|(u, _)| u))
}

pub async fn find_by_email(db: &Db, email: &str) -> Result<Option<User>, AppError> {
    Ok(find_where(db, "email = ?1", email.into())
        .await?
        .map(|(u, _)| u))
}

impl AuthBackend {
    pub fn new(db: Db, config: Arc<Config>) -> Self {
        Self { db, config }
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> i64 {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.auth_hash
    }
}

impl AuthnBackend for AuthBackend {
    type User = User;
    type Credentials = PasswordCredentials;
    type Error = AppError;

    async fn authenticate(&self, creds: PasswordCredentials) -> Result<Option<User>, AppError> {
        // Same generic 401 as a wrong password: the format check must not reveal anything new.
        if crate::validate::email(&creds.email, &()).is_err() {
            return Ok(None);
        }
        let found = find_where(&self.db, "email = ?1", creds.email.as_str().into()).await?;
        let hash = found
            .as_ref()
            .and_then(|(_, h)| h.clone())
            .unwrap_or_else(|| DUMMY_HASH.clone());
        let verified = hash.clone();
        let ok = password::verify(creds.password, hash).await?;
        match found {
            Some((user, Some(_))) if ok && !user.disabled => {
                let conn = self.db.conn().await?;
                promote_if_listed(conn.writer(), &self.config, user.id).await?;
                // Records the promotion, so the read below catches the replica up first.
                drop(conn);
                // A reset/claim/disable committed during argon2 must not yield a session.
                let fresh = find_where(&self.db, "id = ?1", user.id.into()).await?;
                Ok(fresh
                    .filter(|(u, h)| !u.disabled && h.as_deref() == Some(verified.as_str()))
                    .map(|(u, _)| u))
            }
            // Only with the right password, so it reveals nothing to a guesser. Same wording as OAuth.
            Some((user, Some(_))) if ok && user.disabled => Err(AppError::Forbidden(
                "This account has been disabled. Contact the administrator.".into(),
            )),
            _ => Ok(None),
        }
    }

    async fn get_user(&self, id: &UserId<Self>) -> Result<Option<User>, AppError> {
        Ok(find_user(&self.db, *id).await?.filter(|u| !u.disabled))
    }
}

/// On the caller's connection: makes accounts listed in `TIINY_ADMIN_EMAILS` admins.
/// `None` checks every account (startup); returns how many were promoted.
pub async fn promote_listed(
    w: &libsql::Connection,
    config: &Config,
    user_id: Option<i64>,
) -> libsql::Result<u64> {
    if config.admin_emails.is_empty() {
        return Ok(0);
    }
    let listed = serde_json::to_string(&config.admin_emails).expect("strings serialize");
    let id = user_id.map_or(libsql::Value::Null, libsql::Value::Integer);
    w.execute(
        "UPDATE users SET role = 'admin'
         WHERE (?1 IS NULL OR id = ?1) AND role != 'admin'
           AND email IN (SELECT value FROM json_each(?2))",
        libsql::params![id, listed],
    )
    .await
}

/// [`promote_listed`] for one user.
pub async fn promote_if_listed(
    w: &libsql::Connection,
    config: &Config,
    user_id: i64,
) -> libsql::Result<()> {
    promote_listed(w, config, Some(user_id)).await.map(drop)
}
