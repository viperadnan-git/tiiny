use std::{net::IpAddr, sync::Arc, time::Duration};

use axum::http::{HeaderMap, HeaderValue, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use tower_governor::governor::GovernorConfigBuilder;
use tower_sessions::cookie::{Cookie, SameSite, time};

use crate::{
    auth::{email::hash_token, rate_limit_key},
    config::Config,
    db::{Db, now_unix},
    error::AppError,
};

/// `__Host-` on https: browsers refuse it unless Secure, Path=/ and no Domain, so a sibling
/// subdomain can't plant a cookie.
pub fn cookie_name(config: &Config) -> &'static str {
    if config.https() {
        "__Host-tiiny_guest"
    } else {
        "tiiny_guest"
    }
}

/// Hash of the caller's guest secret; None when the cookie is absent or malformed.
pub fn key_hash(config: &Config, headers: &HeaderMap) -> Option<String> {
    let name = cookie_name(config);
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(Cookie::split_parse)
        .flatten()
        .find(|c| c.name() == name)
        .filter(|c| {
            URL_SAFE_NO_PAD
                .decode(c.value())
                .is_ok_and(|b| b.len() == crate::auth::email::SECRET_BYTES)
        })
        .map(|c| hash_token(c.value()))
}

fn cookie(config: &Config, value: String, max_age: time::Duration) -> HeaderValue {
    let c = Cookie::build((cookie_name(config), value))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(config.https())
        .path(if config.https() { "/" } else { "/api/v1" })
        .max_age(max_age)
        .build();
    HeaderValue::from_str(&c.to_string()).expect("cookie is ASCII")
}

pub fn new_cookie(config: &Config) -> (String, HeaderValue) {
    let secret = crate::auth::email::random_secret();
    let header = cookie(config, secret.clone(), time::Duration::seconds(34_560_000));
    (secret, header)
}

/// Returns whether the key may proceed. `tower_governor` doesn't re-export the `governor` crate,
/// so the limiter's type can't be named; it is captured in the closure.
pub type GuestLimiter = Arc<dyn Fn(IpAddr) -> bool + Send + Sync>;

/// Shared by the guest and login limiters: one request per 6 s per key after a burst of 10.
pub(crate) const LIMIT_PERIOD: Duration = Duration::from_secs(6);
pub(crate) const LIMIT_BURST: u32 = 10;

/// Drops a limiter's idle keys every minute; `prune` returns false once the limiter is gone, ending the task.
pub(crate) fn prune_every_minute(prune: impl Fn() -> bool + Send + 'static) {
    if let Ok(rt) = tokio::runtime::Handle::try_current() {
        rt.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                if !prune() {
                    return;
                }
            }
        });
    }
}

pub fn limiter() -> GuestLimiter {
    let config = GovernorConfigBuilder::default()
        .period(LIMIT_PERIOD)
        .burst_size(LIMIT_BURST)
        .finish()
        .expect("non-zero rate limit");
    let l = config.limiter().clone();
    let weak = Arc::downgrade(&l);
    prune_every_minute(move || weak.upgrade().map(|l| l.retain_recent()).is_some());
    Arc::new(move |ip| l.check_key(&ip).is_ok())
}

/// No resolvable IP shares one bucket.
pub fn check(l: &GuestLimiter, ip: Option<IpAddr>) -> Result<(), AppError> {
    if l(ip.map_or(IpAddr::from([0, 0, 0, 0]), rate_limit_key)) {
        Ok(())
    } else {
        Err(AppError::RateLimited { retry_after: 6 })
    }
}

/// Moves the caller's guest links to `user_id`; true when a valid guest cookie was present.
pub async fn claim(
    db: &Db,
    config: &Config,
    user_id: i64,
    headers: &HeaderMap,
) -> Result<bool, AppError> {
    let Some(hash) = key_hash(config, headers) else {
        return Ok(false);
    };
    db.conn()
        .await?
        .execute(
            "UPDATE links SET user_id = ?1, guest_key_hash = NULL, updated_at = ?2
             WHERE guest_key_hash = ?3",
            libsql::params![user_id, now_unix(), hash],
        )
        .await?;
    Ok(true)
}

/// `claim`, returning the headers that clear the cookie when it was present.
pub async fn claim_headers(
    db: &Db,
    config: &Config,
    user_id: i64,
    headers: &HeaderMap,
) -> Result<HeaderMap, AppError> {
    let mut out = HeaderMap::new();
    if claim(db, config, user_id, headers).await? {
        out.insert(
            header::SET_COOKIE,
            cookie(config, String::new(), time::Duration::ZERO),
        );
    }
    Ok(out)
}
