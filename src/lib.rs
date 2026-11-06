pub mod admin;
pub mod auth;
pub mod cache;
pub mod config;
pub mod db;
pub mod error;
pub mod frontend;
pub mod guest;
pub mod links;
pub mod logging;
pub mod redirect;
pub mod reports;
pub mod stats;
pub mod validate;

use std::{net::IpAddr, sync::Arc};

use axum::{
    Router,
    extract::{DefaultBodyLimit, FromRef, FromRequestParts},
    http::request::Parts,
    routing::get,
};
use axum_client_ip::{ClientIp, ClientIpSource, Rejection};
use tower_http::{
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub db: db::Db,
    pub cache: cache::SlugCache,
    pub stats: stats::collector::StatsCollector,
    pub mailer: auth::email::Mailer,
    pub frontend: frontend::Frontend,
    pub guest_limiter: guest::GuestLimiter,
    pub report_limiter: guest::GuestLimiter,
}

impl AppState {
    pub fn new(config: config::Config, db: db::Db) -> anyhow::Result<Self> {
        Ok(Self {
            cache: cache::SlugCache::new(&config),
            stats: stats::collector::StatsCollector::new(&config)?,
            mailer: auth::email::Mailer::from_config(&config)?,
            frontend: frontend::Frontend::new(config.origin()),
            guest_limiter: guest::limiter(),
            report_limiter: guest::limiter(),
            config: Arc::new(config),
            db,
        })
    }
}

/// Context for garde rules that need none.
impl FromRef<AppState> for () {
    fn from_ref(_: &AppState) {}
}

impl FromRef<AppState> for Arc<config::Config> {
    fn from_ref(s: &AppState) -> Self {
        s.config.clone()
    }
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .nest("/auth", auth::router())
        .merge(links::router())
        .merge(admin::router())
        .merge(reports::router())
        .fallback(|| async { error::AppError::NotFound })
        .layer(auth::auth_layer(&state))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(axum::middleware::from_fn(auth::require_json_for_mutations));
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api/v1", api)
        .fallback(frontend::fallback)
        // Outside the session layer, whose saves are writes too.
        .layer(axum::middleware::from_fn_with_state(
            state.db.clone(),
            db::sync_writes,
        ))
        .layer(state.config.client_ip_source.clone().into_extension())
        .layer(CompressionLayer::new())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(logging::request_span)
                .on_request(())
                .on_response(logging::on_response)
                .on_eos(())
                .on_failure(()),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}

pub enum CliOutcome {
    Serve(Box<config::Config>),
    Done(String),
}

#[derive(clap::Parser, Debug)]
#[command(
    version,
    about = "Self-hosted link shortener that counts clicks, not people; configured through TIINY_* environment variables"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run the server (default)
    Serve,
    /// Print a temporary password and drop the account's sessions and linked identities
    ResetPassword {
        #[arg(value_parser = email_arg)]
        email: String,
    },
    /// Make an existing user an admin
    PromoteAdmin {
        #[arg(value_parser = email_arg)]
        email: String,
    },
}

fn email_arg(raw: &str) -> Result<String, String> {
    validate::parse_email(raw).map_err(|e| e.message().to_owned())
}

pub async fn run_cli(cli: Cli, config: config::Config) -> anyhow::Result<CliOutcome> {
    let (email, promote) = match cli.command {
        None | Some(Command::Serve) => return Ok(CliOutcome::Serve(Box::new(config))),
        Some(Command::ResetPassword { email }) => (email, false),
        Some(Command::PromoteAdmin { email }) => (email, true),
    };
    let db = db::Db::open_direct(&config).await?;
    db.migrate().await?;
    let user = auth::backend::find_by_email(&db, &email)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no user with email {email}"))?;
    if promote {
        db.conn()
            .await?
            .execute("UPDATE users SET role = 'admin' WHERE id = ?1", [user.id])
            .await?;
        return Ok(CliOutcome::Done(format!("{email} is now an admin")));
    }
    let password = links::slug::random_base62(16);
    let hash = auth::password::hash_password(&password)?;
    let conn = db.conn().await?;
    let tx = conn.transaction().await?;
    auth::recover_account(&tx, user.id, &hash, false).await?;
    tx.commit().await?;
    Ok(CliOutcome::Done(format!(
        "temporary password for {email}: {password}"
    )))
}

/// Where short links live: `TIINY_BASE_URL`, else `http://` + the request's Host (unconfigured
/// instances). Only for responses to that request; emails and OAuth use the configured origin.
#[derive(Debug, Clone)]
pub struct ShortOrigin {
    origin: String,
    /// Host the request was sent to, lowercased, port and trailing dot dropped.
    pub host: Option<String>,
}

impl ShortOrigin {
    pub fn short_url(&self, slug: &str) -> String {
        format!("{}/{slug}", self.origin)
    }
}

impl FromRequestParts<AppState> for ShortOrigin {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self, Self::Rejection> {
        let authority = parts
            .headers
            .get(axum::http::header::HOST)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.parse::<axum::http::uri::Authority>().ok())
            .or_else(|| parts.uri.authority().cloned());
        let origin = match (s.config.origin(), &authority) {
            ("", Some(a)) => format!("http://{a}"),
            (o, _) => o.to_owned(),
        };
        Ok(Self {
            origin,
            host: authority.map(|a| a.host().trim_end_matches('.').to_ascii_lowercase()),
        })
    }
}

/// Client IP from the configured [`ClientIpSource`] (axum-client-ip); `None` when that header or the peer
/// address is missing or unparsable, so a bad header value never fails the request. A missing
/// `ClientIpSource` extension is a wiring bug: 500.
#[derive(Debug, Clone, Copy)]
pub struct ClientAddr(pub Option<IpAddr>);

impl<S: Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = error::AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, error::AppError> {
        let ip = match ClientIp::from_request_parts(parts, state).await {
            Ok(ClientIp(ip)) => Some(ip),
            Err(e @ Rejection::NoClientIpSource) => {
                return Err(error::AppError::Internal(anyhow::anyhow!(
                    "client ip: {e} (build_router adds it)"
                )));
            }
            // Missing/invalid header (client-controlled) or no ConnectInfo (in-process tests): no IP, don't fail.
            Err(_) => None,
        };
        if parts.extensions.get() == Some(&ClientIpSource::ConnectInfo)
            && ip.is_some_and(is_proxy_like)
        {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                tracing::warn!(
                    "request from loopback/private address with TIINY_CLIENT_IP_HEADER unset; \
                     behind a reverse proxy all clients share one rate-limit bucket and GeoIP sees the proxy"
                )
            });
        }
        Ok(Self(ip))
    }
}

fn is_proxy_like(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_proxy_like(v4.into()),
            None => v6.is_loopback() || v6.is_unique_local(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn extract(
        source: Option<ClientIpSource>,
        headers: &[(&str, &str)],
    ) -> Result<ClientAddr, error::AppError> {
        let mut req = axum::http::Request::builder();
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let (mut parts, ()) = req.body(()).unwrap().into_parts();
        if let Some(source) = source {
            parts.extensions.insert(source);
        }
        let peer = std::net::SocketAddr::from(([9, 9, 9, 9], 1));
        parts.extensions.insert(axum::extract::ConnectInfo(peer));
        ClientAddr::from_request_parts(&mut parts, &()).await
    }

    async fn addr(source: ClientIpSource, headers: &[(&str, &str)]) -> Option<IpAddr> {
        extract(Some(source), headers).await.unwrap().0
    }

    #[tokio::test]
    async fn client_addr_without_source_is_internal_error() {
        assert!(matches!(
            extract(None, &[("x-real-ip", "1.2.3.4")]).await,
            Err(error::AppError::Internal(_))
        ));
    }

    #[tokio::test]
    async fn client_addr_uses_configured_source() {
        use ClientIpSource::*;
        let ip = |s: &str| Some(s.parse::<IpAddr>().unwrap());
        let all = [
            ("cf-connecting-ip", "1.1.1.1"),
            ("x-real-ip", "2.2.2.2"),
            ("x-forwarded-for", "6.6.6.6, 3.3.3.3"),
        ];
        assert_eq!(addr(CfConnectingIp, &all).await, ip("1.1.1.1"));
        assert_eq!(addr(XRealIp, &all).await, ip("2.2.2.2"));
        assert_eq!(addr(RightmostXForwardedFor, &all).await, ip("3.3.3.3"));
        assert_eq!(addr(ConnectInfo, &all).await, ip("9.9.9.9"));
        // A proxy may append its own header line rather than extend the existing one.
        let two_lines = [
            ("x-forwarded-for", "6.6.6.6"),
            ("x-forwarded-for", "5.5.5.5, 4.4.4.4"),
        ];
        assert_eq!(
            addr(RightmostXForwardedFor, &two_lines).await,
            ip("4.4.4.4")
        );
    }

    #[tokio::test]
    async fn client_addr_bad_or_missing_header_is_none() {
        use ClientIpSource::*;
        assert_eq!(addr(XRealIp, &[("x-real-ip", "garbage")]).await, None);
        assert_eq!(addr(XRealIp, &[]).await, None);
        assert_eq!(
            addr(RightmostXForwardedFor, &[("x-forwarded-for", "1.2.3.4, x")]).await,
            None
        );
        assert_eq!(
            addr(CfConnectingIp, &[("x-real-ip", "2.2.2.2")]).await,
            None
        );
    }

    #[test]
    fn proxy_like_peers() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "::1",
            "fd00::1",
            "::ffff:172.16.0.1",
        ] {
            assert!(is_proxy_like(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "2001:db8::1", "::ffff:8.8.8.8"] {
            assert!(!is_proxy_like(ip.parse().unwrap()), "{ip}");
        }
    }
}
