use std::{path::PathBuf, time::Duration};

use axum_client_ip::ClientIpSource;
use figment::{Figment, providers::Serialized};
use serde::{Deserialize, Deserializer, de::Error as _};

#[derive(Debug, Clone)]
pub struct OauthClientConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpTls {
    Starttls,
    Tls,
    None,
}

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
    pub tls: SmtpTls,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// None: emails and OAuth are off and pages use relative URLs.
    pub base_url: Option<url::Url>,
    /// IP or hostname to listen on.
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub database_url: Option<String>,
    pub database_auth_token: Option<String>,
    pub replica_sync_interval: Duration,
    pub cache_ttl: Duration,
    pub cache_max_entries: u64,
    pub stats_flush_interval: Duration,
    /// Whole days.
    pub stats_retention_days: Option<u32>,
    pub report_email_interval: Duration,
    pub max_links_per_user_per_day: Option<u32>,
    pub blocked_domains: Vec<String>,
    pub signup_enabled: bool,
    pub guest_links_enabled: bool,
    pub max_guest_links_per_ip_per_day: u32,
    pub password_auth_enabled: bool,
    pub admin_emails: Vec<String>,
    pub google: Option<OauthClientConfig>,
    pub github: Option<OauthClientConfig>,
    pub smtp: Option<SmtpConfig>,
    /// From `TIINY_CLIENT_IP_HEADER`; the TCP peer when unset.
    pub client_ip_source: ClientIpSource,
    pub geoip_db_path: Option<PathBuf>,
    /// `EnvFilter` directives; None uses `logging::DEFAULT_FILTER`.
    pub log: Option<String>,
    /// Also write JSON lines to `<data_dir>/logs`, one file per day, a week kept.
    pub log_file: bool,
}

/// `TIINY_*` variables (plus `HOST` and `PORT`), prefix stripped and lowercased.
#[derive(Default, Deserialize, garde::Validate)]
#[serde(default)]
struct Vars {
    #[garde(custom(origin(&self.smtp_host, &self.google_client_id, &self.github_client_id)))]
    base_url: Option<url::Url>,
    #[garde(skip)]
    host: Option<String>,
    #[garde(skip)]
    port: Option<u16>,
    #[garde(skip)]
    data_dir: Option<PathBuf>,
    #[garde(custom(required_with(&self.database_auth_token, "required when TIINY_DATABASE_AUTH_TOKEN is set")))]
    database_url: Option<String>,
    #[garde(custom(required_with(&self.database_url, "required when TIINY_DATABASE_URL is set")))]
    database_auth_token: Option<String>,
    #[serde(deserialize_with = "duration")]
    #[garde(inner(custom(positive)))]
    replica_sync_interval: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    #[garde(skip)]
    cache_ttl: Option<Duration>,
    #[garde(skip)]
    cache_max_entries: Option<u64>,
    #[serde(deserialize_with = "duration")]
    #[garde(inner(custom(positive)))]
    stats_flush_interval: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    #[garde(inner(custom(at_least_a_day)))]
    stats_retention: Option<Duration>,
    #[serde(deserialize_with = "duration")]
    #[garde(inner(custom(positive)))]
    report_email_interval: Option<Duration>,
    #[garde(skip)]
    max_links_per_user_per_day: Option<u32>,
    #[garde(skip)]
    blocked_domains: Option<String>,
    #[garde(skip)]
    signup_enabled: Option<bool>,
    #[garde(skip)]
    guest_links_enabled: Option<bool>,
    #[garde(skip)]
    max_guest_links_per_ip_per_day: Option<u32>,
    #[garde(skip)]
    password_auth_enabled: Option<bool>,
    #[garde(skip)]
    admin_emails: Option<String>,
    #[garde(custom(required_with(&self.google_client_secret, "required when client secret is set")))]
    google_client_id: Option<String>,
    #[garde(custom(required_with(&self.google_client_id, "required when client id is set")))]
    google_client_secret: Option<String>,
    #[garde(custom(required_with(&self.github_client_secret, "required when client secret is set")))]
    github_client_id: Option<String>,
    #[garde(custom(required_with(&self.github_client_id, "required when client id is set")))]
    github_client_secret: Option<String>,
    #[garde(skip)]
    smtp_host: Option<String>,
    #[garde(skip)]
    smtp_port: Option<u16>,
    #[garde(skip)]
    smtp_username: Option<String>,
    #[garde(custom(required_with(&self.smtp_username, "required when TIINY_SMTP_USERNAME is set")))]
    smtp_password: Option<String>,
    #[garde(custom(required_with(&self.smtp_host, "required when TIINY_SMTP_HOST is set")))]
    smtp_from: Option<String>,
    #[serde(deserialize_with = "lowercase")]
    #[garde(skip)]
    smtp_tls: Option<SmtpTls>,
    #[serde(deserialize_with = "ip_source")]
    #[garde(skip)]
    client_ip_header: Option<ClientIpSource>,
    #[garde(skip)]
    geoip_db_path: Option<PathBuf>,
    #[garde(inner(custom(log_filter)))]
    log: Option<String>,
    #[garde(skip)]
    log_file: Option<bool>,
}

fn log_filter(v: &str, _: &()) -> garde::Result {
    tracing_subscriber::EnvFilter::try_new(v)
        .map(|_| ())
        .map_err(|e| garde::Error::new(e.to_string()))
}

/// Required when SMTP or OAuth is on: email links and OAuth callbacks can't trust the request's Host.
fn origin<'a>(
    smtp: &'a Option<String>,
    google: &'a Option<String>,
    github: &'a Option<String>,
) -> impl FnOnce(&Option<url::Url>, &()) -> garde::Result + 'a {
    move |u, _| {
        let Some(u) = u else {
            return match (smtp, google, github) {
                (None, None, None) => Ok(()),
                _ => Err(garde::Error::new(
                    "required when TIINY_SMTP_HOST or an OAuth client is set",
                )),
            };
        };
        check_origin(u)
    }
}

fn check_origin(u: &url::Url) -> garde::Result {
    if !matches!(u.scheme(), "http" | "https")
        || u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err(garde::Error::new(
            "must be an http(s) origin without userinfo, path, query or fragment",
        ));
    }
    // The origin is written verbatim into HTML attributes and JSON-LD.
    let ldh = |d: &str| {
        d.split('.')
            .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
    };
    match u.host() {
        Some(url::Host::Ipv4(_) | url::Host::Ipv6(_)) => Ok(()),
        Some(url::Host::Domain(d)) if ldh(d) => Ok(()),
        _ => Err(garde::Error::new(
            "host must be an IP address or a domain of letters, digits, '-' and '.'",
        )),
    }
}

fn required_with<'a, T, U>(
    other: &'a Option<U>,
    msg: &'static str,
) -> impl FnOnce(&Option<T>, &()) -> garde::Result + 'a {
    move |v, _| match (v, other) {
        (None, Some(_)) => Err(garde::Error::new(msg)),
        _ => Ok(()),
    }
}

fn positive(v: &Duration, _: &()) -> garde::Result {
    if v.is_zero() {
        Err(garde::Error::new("must be longer than 0"))
    } else {
        Ok(())
    }
}

fn at_least_a_day(v: &Duration, _: &()) -> garde::Result {
    if v.as_secs() < 86_400 {
        Err(garde::Error::new(
            "must be at least 1d (stats are kept per day)",
        ))
    } else {
        Ok(())
    }
}

/// Seconds as a bare number (`60`), else humantime's format (`90s`, `10m`, `1h30m`, `7d`).
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    match s.parse::<u64>() {
        Ok(secs) => Ok(Duration::from_secs(secs)),
        Err(_) => humantime::parse_duration(s)
            .map_err(|e| format!("{e}; use seconds (60) or a duration like 90s, 10m, 1h30m or 7d")),
    }
}

fn duration<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|s| parse_duration(&s).map_err(D::Error::custom))
        .transpose()
}

/// Case-insensitive enum value.
fn lowercase<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Option::<String>::deserialize(d)?
        .map(|s| {
            T::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                s.to_ascii_lowercase(),
            ))
        })
        .transpose()
}

/// Header name (case-insensitive) to the axum-client-ip source that parses it.
fn ip_source<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ClientIpSource>, D::Error> {
    let Some(h) = Option::<String>::deserialize(d)? else {
        return Ok(None);
    };
    Ok(Some(match h.to_ascii_lowercase().as_str() {
        "cf-connecting-ip" => ClientIpSource::CfConnectingIp,
        "x-real-ip" => ClientIpSource::XRealIp,
        "x-forwarded-for" => ClientIpSource::RightmostXForwardedFor,
        "true-client-ip" => ClientIpSource::TrueClientIp,
        "fly-client-ip" => ClientIpSource::FlyClientIp,
        "cloudfront-viewer-address" => ClientIpSource::CloudFrontViewerAddress,
        "x-envoy-external-address" => ClientIpSource::XEnvoyExternalAddress,
        _ => {
            return Err(D::Error::custom(format!(
                "unsupported header {h:?}; use CF-Connecting-IP, X-Real-IP, X-Forwarded-For, \
                 True-Client-IP, Fly-Client-IP, CloudFront-Viewer-Address or X-Envoy-External-Address"
            )));
        }
    }))
}

fn list(v: Option<String>) -> Vec<String> {
    v.iter()
        .flat_map(|v| v.split(','))
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Renamed variables, `(old name, what replaces it)` with the prefix stripped and lowercase: startup
/// fails on the old name instead of ignoring it. Empty until a release renames one.
const RENAMED: [(&str, &str); 0] = [];

/// Read without the `TIINY_` prefix, as most platforms set them.
const UNPREFIXED: [&str; 2] = ["host", "port"];

fn var_name(path: impl std::fmt::Display) -> String {
    let path = path.to_string();
    match UNPREFIXED.contains(&path.as_str()) {
        true => path.to_ascii_uppercase(),
        false => format!("TIINY_{}", path.to_ascii_uppercase()),
    }
}

impl Config {
    /// Guests can shorten links: switched on and a non-zero daily cap.
    pub fn guest_links(&self) -> bool {
        self.guest_links_enabled && self.max_guest_links_per_ip_per_day > 0
    }

    /// `base_url` without its trailing slash; empty when unset.
    pub fn origin(&self) -> &str {
        self.base_url
            .as_ref()
            .map_or("", |u| u.as_str().trim_end_matches('/'))
    }

    pub fn https(&self) -> bool {
        self.base_url
            .as_ref()
            .is_some_and(|u| u.scheme() == "https")
    }

    pub fn from_env() -> Result<Config, String> {
        Self::from_vars(
            std::env::vars_os()
                .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))),
        )
    }

    /// `TIINY_*` pairs plus `HOST` and `PORT`; other names are ignored.
    pub fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Result<Config, String> {
        Self::from_pairs(vars.into_iter().filter_map(|(k, v)| {
            let key = match k.strip_prefix("TIINY_") {
                Some(rest) => rest.to_ascii_lowercase(),
                None => k.to_ascii_lowercase(),
            };
            let prefixed = k.starts_with("TIINY_");
            (prefixed != UNPREFIXED.contains(&key.as_str())).then_some((key, v))
        }))
    }

    /// Values stay strings (figment's `Env` would turn `"0123"` into a number); `extract_lossy` converts them
    /// per field. Blank values count as unset.
    fn from_pairs(pairs: impl Iterator<Item = (String, String)>) -> Result<Config, String> {
        let pairs: std::collections::BTreeMap<String, String> = pairs
            .map(|(k, v)| (k, v.trim().to_owned()))
            .filter(|(_, v)| !v.is_empty())
            .collect();
        // Unknown vars are ignored, so a renamed one would silently fall back to its default.
        if let Some((old, new)) = RENAMED.iter().find(|(old, _)| pairs.contains_key(*old)) {
            return Err(format!("{}: replaced by {new}", var_name(old)));
        }
        let v: Vars = Figment::from(Serialized::defaults(pairs))
            .extract_lossy()
            .map_err(|e| match e.path.is_empty() {
                true => e.kind.to_string(),
                false => format!("{}: {}", var_name(e.path.join("_")), e.kind),
            })?;
        garde::Validate::validate(&v).map_err(|r| {
            let (path, e) = r.iter().next().expect("non-empty report");
            format!("{}: {}", var_name(path), e.message())
        })?;
        let oauth = |id: Option<String>, secret: Option<String>| {
            Some(OauthClientConfig {
                client_id: id?,
                client_secret: secret?,
            })
        };
        let tls = v.smtp_tls.unwrap_or(SmtpTls::Starttls);
        let smtp = v.smtp_host.map(|host| SmtpConfig {
            host,
            port: v
                .smtp_port
                .unwrap_or(if tls == SmtpTls::Tls { 465 } else { 587 }),
            username: v.smtp_username,
            password: v.smtp_password,
            from: v.smtp_from.expect("validated: required with host"),
            tls,
        });
        Ok(Config {
            base_url: v.base_url,
            host: v.host.unwrap_or_else(|| "0.0.0.0".into()),
            port: v.port.unwrap_or(8080),
            data_dir: v.data_dir.unwrap_or_else(|| "./.data".into()),
            database_url: v.database_url,
            database_auth_token: v.database_auth_token,
            replica_sync_interval: v.replica_sync_interval.unwrap_or(Duration::from_secs(5)),
            cache_ttl: v.cache_ttl.unwrap_or(Duration::from_secs(60)),
            cache_max_entries: v.cache_max_entries.unwrap_or(100_000),
            stats_flush_interval: v.stats_flush_interval.unwrap_or(Duration::from_secs(10)),
            stats_retention_days: v.stats_retention.map(|d| (d.as_secs() / 86_400) as u32),
            report_email_interval: v.report_email_interval.unwrap_or(Duration::from_secs(3600)),
            max_links_per_user_per_day: v.max_links_per_user_per_day,
            // Punycode, as `Url::host_str` gives the link's host: `bücher.example` -> `xn--bcher-kva.example`.
            blocked_domains: list(v.blocked_domains)
                .into_iter()
                .map(|d| d.trim_end_matches('.').to_owned())
                .filter(|d| !d.is_empty())
                .map(|d| url::Host::parse(&d).map_or(d, |h| h.to_string()))
                .collect(),
            signup_enabled: v.signup_enabled.unwrap_or(true),
            guest_links_enabled: v.guest_links_enabled.unwrap_or(true),
            max_guest_links_per_ip_per_day: v.max_guest_links_per_ip_per_day.unwrap_or(20),
            password_auth_enabled: v.password_auth_enabled.unwrap_or(true),
            admin_emails: list(v.admin_emails),
            google: oauth(v.google_client_id, v.google_client_secret),
            github: oauth(v.github_client_id, v.github_client_secret),
            smtp,
            client_ip_source: v.client_ip_header.unwrap_or(ClientIpSource::ConnectInfo),
            geoip_db_path: v.geoip_db_path,
            log: v.log,
            log_file: v.log_file.unwrap_or(true),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_vars(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())))
    }

    fn err(pairs: &[(&str, &str)]) -> String {
        cfg(pairs).unwrap_err().to_string()
    }

    #[test]
    fn defaults() {
        let c = cfg(&[("TIINY_BASE_URL", "https://link.example")]).unwrap();
        assert_eq!((c.host.as_str(), c.port), ("0.0.0.0", 8080));
        assert_eq!(c.data_dir, PathBuf::from("./.data"));
        assert_eq!(c.replica_sync_interval, Duration::from_secs(5));
        assert!(c.guest_links_enabled);
        assert_eq!(c.max_guest_links_per_ip_per_day, 20);
        assert_eq!(c.cache_ttl, Duration::from_secs(60));
        assert_eq!(c.cache_max_entries, 100000);
        assert_eq!(c.stats_flush_interval, Duration::from_secs(10));
        assert_eq!(c.stats_retention_days, None);
        assert_eq!(c.report_email_interval, Duration::from_secs(3600));
        assert_eq!(c.max_links_per_user_per_day, None);
        assert!(c.signup_enabled && c.password_auth_enabled);
        assert!(c.google.is_none() && c.github.is_none() && c.smtp.is_none());
    }

    #[test]
    fn log_filter_validated() {
        let c = cfg(&[]).unwrap();
        assert_eq!((c.log, c.log_file), (None, true));
        let c = cfg(&[
            ("TIINY_LOG", "warn,tiiny=debug"),
            ("TIINY_LOG_FILE", "false"),
        ])
        .unwrap();
        assert_eq!(
            (c.log.as_deref(), c.log_file),
            (Some("warn,tiiny=debug"), false)
        );
        assert!(err(&[("TIINY_LOG", "tiiny=loud")]).starts_with("TIINY_LOG: "));
        assert!(log_filter(crate::logging::DEFAULT_FILTER, &()).is_ok());
    }

    #[test]
    fn base_url_optional_unless_email_or_oauth() {
        let c = cfg(&[]).unwrap();
        assert_eq!((c.origin(), c.https()), ("", false));
        for k in [
            "TIINY_SMTP_HOST",
            "TIINY_GOOGLE_CLIENT_ID",
            "TIINY_GITHUB_CLIENT_ID",
        ] {
            let id = k.replace("_ID", "_SECRET");
            let e = err(&[(k, "x"), (&id, "x"), ("TIINY_SMTP_FROM", "a@b.c")]);
            assert!(e.starts_with("TIINY_BASE_URL: required"), "{k}: {e}");
        }
    }

    #[test]
    fn base_url_shape() {
        for u in [
            "ftp://link.example",
            "https://link.example/app",
            "https://link.example/?a=1",
            "https://link.example/#x",
        ] {
            assert!(
                err(&[("TIINY_BASE_URL", u)]).contains("TIINY_BASE_URL"),
                "{u}"
            );
        }
        assert!(cfg(&[("TIINY_BASE_URL", "http://localhost:8080/")]).is_ok());
        assert!(cfg(&[("TIINY_BASE_URL", "http://[::1]:8080")]).is_ok());
        assert!(cfg(&[("TIINY_BASE_URL", "https://xn--bcher-kva.example")]).is_ok());
    }

    #[test]
    fn base_url_host_rejects_markup_characters() {
        for u in [
            "https://a\"onmouseover=alert(1)\"b.com/",
            "https://a'b.com",
            "https://a<b>.com",
            "https://a`b.com",
            "https://a&b.com",
            "https://a{b}.com",
        ] {
            assert!(
                err(&[("TIINY_BASE_URL", u)]).contains("TIINY_BASE_URL"),
                "{u}"
            );
        }
    }

    #[test]
    fn oauth_pair_required() {
        let e = err(&[
            ("TIINY_BASE_URL", "https://link.example"),
            ("TIINY_GOOGLE_CLIENT_ID", "id"),
        ]);
        assert!(e.contains("TIINY_GOOGLE_CLIENT_SECRET"));
    }

    #[test]
    fn lists_are_normalised() {
        let c = cfg(&[
            ("TIINY_BASE_URL", "https://link.example"),
            ("TIINY_ADMIN_EMAILS", " A@x.com, b@Y.com "),
            ("TIINY_BLOCKED_DOMAINS", "Evil.com,,x.org"),
        ])
        .unwrap();
        assert_eq!(c.admin_emails, ["a@x.com", "b@y.com"]);
        assert_eq!(c.blocked_domains, ["evil.com", "x.org"]);
    }

    #[test]
    fn blocked_domains_in_punycode() {
        let c = Config::from_vars([
            ("TIINY_BASE_URL".into(), "https://t.test".into()),
            ("TIINY_BLOCKED_DOMAINS".into(), "Bücher.Example".into()),
        ])
        .unwrap();
        assert_eq!(c.blocked_domains, ["xn--bcher-kva.example"]);
    }

    #[test]
    fn smtp_rules() {
        assert!(
            err(&[
                ("TIINY_BASE_URL", "https://link.example"),
                ("TIINY_SMTP_HOST", "h"),
                ("TIINY_SMTP_FROM", "a@b.c"),
                ("TIINY_SMTP_USERNAME", "u"),
            ])
            .contains("TIINY_SMTP_PASSWORD")
        );
        let base = ("TIINY_BASE_URL", "https://link.example");
        assert!(err(&[base, ("TIINY_SMTP_HOST", "h")]).contains("TIINY_SMTP_FROM"));
        let s = cfg(&[base, ("TIINY_SMTP_HOST", "h"), ("TIINY_SMTP_FROM", "a@b.c")])
            .unwrap()
            .smtp
            .unwrap();
        assert_eq!((s.tls, s.port), (SmtpTls::Starttls, 587));
        let s = cfg(&[
            base,
            ("TIINY_SMTP_HOST", "h"),
            ("TIINY_SMTP_FROM", "a@b.c"),
            ("TIINY_SMTP_TLS", "tls"),
        ])
        .unwrap()
        .smtp
        .unwrap();
        assert_eq!((s.tls, s.port), (SmtpTls::Tls, 465));
    }

    #[test]
    fn database_pair_and_sync_interval() {
        let base = ("TIINY_BASE_URL", "https://link.example");
        assert!(
            err(&[base, ("TIINY_DATABASE_URL", "libsql://x")])
                .contains("TIINY_DATABASE_AUTH_TOKEN")
        );
        assert!(err(&[base, ("TIINY_DATABASE_AUTH_TOKEN", "t")]).contains("TIINY_DATABASE_URL"));
        assert!(
            err(&[base, ("TIINY_REPLICA_SYNC_INTERVAL", "0s")])
                .contains("TIINY_REPLICA_SYNC_INTERVAL")
        );
        assert!(
            cfg(&[
                base,
                ("TIINY_DATABASE_URL", "libsql://x"),
                ("TIINY_DATABASE_AUTH_TOKEN", "t")
            ])
            .is_ok()
        );
    }

    #[test]
    fn base_url_rejects_userinfo() {
        for u in ["https://u@link.example", "https://u:p@link.example"] {
            assert!(
                err(&[("TIINY_BASE_URL", u)]).contains("TIINY_BASE_URL"),
                "{u}"
            );
        }
    }

    #[test]
    fn parse_errors_name_the_var() {
        let base = ("TIINY_BASE_URL", "https://link.example");
        for (k, v) in [
            ("TIINY_CACHE_TTL", "abc"),
            ("TIINY_REPORT_EMAIL_INTERVAL", "10 lightyears"),
            ("PORT", "99999"),
            ("TIINY_SIGNUP_ENABLED", "maybe"),
            ("TIINY_SMTP_TLS", "ssl"),
            ("TIINY_STATS_RETENTION", "-1"),
            ("TIINY_CLIENT_IP_HEADER", "x-test-ip"),
        ] {
            let e = err(&[base, (k, v)]);
            assert!(e.starts_with(&format!("{k}: ")), "{k}={v}: {e}");
        }
        assert!(err(&[("TIINY_BASE_URL", "not a url")]).starts_with("TIINY_BASE_URL: "));
        let c = |vars: [(&str, &str); 2]| {
            Config::from_vars(vars.map(|(k, v)| (k.into(), v.into()))).unwrap()
        };
        let set = c([("HOST", "localhost"), ("PORT", "9000")]);
        assert_eq!((set.host.as_str(), set.port), ("localhost", 9000));
        let prefixed = c([("TIINY_HOST", "10.0.0.1"), ("TIINY_PORT", "9000")]);
        assert_eq!((prefixed.host.as_str(), prefixed.port), ("0.0.0.0", 8080));
    }

    #[test]
    fn durations_take_seconds_or_units() {
        let base = ("TIINY_BASE_URL", "https://link.example");
        let ttl = |v: &str| cfg(&[base, ("TIINY_CACHE_TTL", v)]).map(|c| c.cache_ttl.as_secs());
        for (v, secs) in [
            ("60", 60),
            ("2s", 2),
            ("1m", 60),
            ("1h", 3600),
            ("1m3h", 10_860),
            ("1h 30m", 5400),
            ("7d", 604_800),
        ] {
            assert_eq!(ttl(v), Ok(secs), "{v}");
        }
        assert!(ttl("1x").unwrap_err().starts_with("TIINY_CACHE_TTL: "));
        let keep =
            |v: &str| cfg(&[base, ("TIINY_STATS_RETENTION", v)]).map(|c| c.stats_retention_days);
        assert_eq!(keep("90d"), Ok(Some(90)));
        assert_eq!(keep("36h"), Ok(Some(1)));
        assert!(keep("12h").unwrap_err().contains("at least 1d"));
        assert!(err(&[base, ("TIINY_REPORT_EMAIL_INTERVAL", "0")]).contains("longer than 0"));
    }

    #[test]
    fn values_are_trimmed_and_blank_is_unset() {
        let c = cfg(&[
            ("TIINY_BASE_URL", " https://link.example "),
            ("TIINY_CACHE_TTL", " 7 "),
            ("TIINY_STATS_RETENTION", ""),
            ("TIINY_SIGNUP_ENABLED", "FALSE"),
            ("TIINY_PASSWORD_AUTH_ENABLED", "0"),
            ("TIINY_GUEST_LINKS_ENABLED", "no"),
            ("TIINY_MAX_GUEST_LINKS_PER_IP_PER_DAY", "3"),
            ("TIINY_SMTP_HOST", "  "),
            ("TIINY_SMTP_TLS", "TLS"),
            // Digit-only strings stay strings.
            ("TIINY_GITHUB_CLIENT_ID", "0123"),
            ("TIINY_GITHUB_CLIENT_SECRET", "4567"),
        ])
        .unwrap();
        assert_eq!(c.cache_ttl, Duration::from_secs(7));
        assert!(!c.guest_links_enabled);
        assert_eq!(c.max_guest_links_per_ip_per_day, 3);
        assert_eq!(c.stats_retention_days, None);
        assert!(!c.signup_enabled && !c.password_auth_enabled);
        assert!(c.smtp.is_none());
        let gh = c.github.unwrap();
        assert_eq!(
            (gh.client_id.as_str(), gh.client_secret.as_str()),
            ("0123", "4567")
        );
    }

    #[test]
    fn client_ip_header_selects_source() {
        let src = |h: Option<&str>| {
            let mut v = vec![("TIINY_BASE_URL", "https://link.example")];
            v.extend(h.map(|h| ("TIINY_CLIENT_IP_HEADER", h)));
            cfg(&v).unwrap().client_ip_source
        };
        assert_eq!(src(None), ClientIpSource::ConnectInfo);
        assert_eq!(
            src(Some("CF-Connecting-IP")),
            ClientIpSource::CfConnectingIp
        );
        assert_eq!(src(Some("x-real-ip")), ClientIpSource::XRealIp);
        assert_eq!(
            src(Some("X-Forwarded-For")),
            ClientIpSource::RightmostXForwardedFor
        );
    }

    #[test]
    fn blocked_domains_drop_trailing_dot() {
        let c = cfg(&[
            ("TIINY_BASE_URL", "https://link.example"),
            ("TIINY_BLOCKED_DOMAINS", "evil.com., x.org"),
        ])
        .unwrap();
        assert_eq!(c.blocked_domains, ["evil.com", "x.org"]);
    }
}
