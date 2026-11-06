//! Input rules shared by handlers and the CLI; `web/src/lib/schemas.ts` (zod) mirrors them.
//!
//! Request bodies normalise during deserialisation (`deserialize_with` helpers below), then garde
//! runs the rules. garde's built-in rules have fixed messages, so the rules are `custom` fns.

use std::sync::LazyLock;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use garde::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Deserializer};

use crate::{
    auth::{
        email::SECRET_BYTES,
        password::{MAX_LEN, MIN_LEN},
    },
    config::Config,
    error::AppError,
};

pub const EMAIL_MAX: usize = 254;
pub const EMAIL_LOCAL_MAX: usize = 64;
pub const NAME_MAX: usize = 100;
pub const QUERY_MAX: usize = 200;
pub const URL_MAX: usize = 2048;

/// Trimmed, lowercased.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Unicode `White_Space` runs collapse to one space (as `split_whitespace` does; the client uses `\p{White_Space}`).
pub fn normalize_name(raw: Option<&str>) -> String {
    raw.unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn de_email<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    Ok(normalize_email(&String::deserialize(d)?))
}

/// Missing or null becomes "", which the `name` rule rejects as required.
pub fn de_name<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    Ok(normalize_name(Option::<String>::deserialize(d)?.as_deref()))
}

/// Trimmed; empty means no filter.
pub fn de_query<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(Option::<String>::deserialize(d)?
        .map(|q| q.trim().to_owned())
        .filter(|q| !q.is_empty()))
}

/// Normalised address (see [`normalize_email`]).
pub fn email<C>(email: &str, _: &C) -> Result {
    let ok = email.len() <= EMAIL_MAX
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && local.len() <= EMAIL_LOCAL_MAX
                && !local.chars().any(|c| c.is_whitespace() || c.is_control())
                && domain.contains('.')
                && domain.split('.').all(|l| {
                    !l.is_empty()
                        && !l.starts_with('-')
                        && !l.ends_with('-')
                        && l.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                })
        });
    if ok {
        Ok(())
    } else {
        Err(Error::new("invalid email address"))
    }
}

/// Normalise + check, for inputs outside a request body (CLI, OAuth, login).
pub fn parse_email(raw: &str) -> std::result::Result<String, Error> {
    let e = normalize_email(raw);
    email(&e, &()).map(|()| e)
}

pub fn password<C>(pw: &str, _: &C) -> Result {
    if !(MIN_LEN..=MAX_LEN).contains(&pw.chars().count()) {
        return Err(Error::new(format!(
            "password must be {MIN_LEN}-{MAX_LEN} characters"
        )));
    }
    if pw.trim().is_empty() {
        return Err(Error::new("password must not be only whitespace"));
    }
    Ok(())
}

static NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\p{L}[\p{L}\p{M}'’ .-]*$").expect("name regex"));

/// Normalised name (see [`normalize_name`]).
pub fn name<C>(name: &str, _: &C) -> Result {
    if name.is_empty() {
        return Err(Error::new("name is required"));
    }
    if name.chars().count() > NAME_MAX {
        return Err(Error::new("name must be at most 100 characters"));
    }
    if !NAME.is_match(name) {
        return Err(Error::new(
            "name must start with a letter and contain only letters, spaces, apostrophes, periods and hyphens",
        ));
    }
    Ok(())
}

/// Exactly what `issue_token` emits (canonical unpadded base64url of `SECRET_BYTES`), so junk never reaches the DB.
/// Not a garde rule: a malformed token gets the same 400 as an expired one, not a 422.
pub fn token(raw: &str) -> std::result::Result<(), AppError> {
    // The decoder rejects padding and non-zero trailing bits, so only the canonical 43-char form passes.
    if URL_SAFE_NO_PAD
        .decode(raw)
        .is_ok_and(|b| b.len() == SECRET_BYTES)
    {
        Ok(())
    } else {
        Err(AppError::BadRequest("invalid or expired token".into()))
    }
}

pub const REPORT_REASONS: [&str; 5] = ["phishing", "malware", "spam", "illegal", "other"];
pub const REPORT_DETAILS_MAX: usize = 2000;

pub fn report_reason<C>(r: &str, _: &C) -> Result {
    if REPORT_REASONS.contains(&r) {
        Ok(())
    } else {
        Err(Error::new(format!(
            "reason must be one of {}",
            REPORT_REASONS.join(", ")
        )))
    }
}

pub fn report_contact_email<C>(e: &str, c: &C) -> Result {
    email(e, c).map_err(|_| Error::new("contact email is invalid"))
}

/// Trimmed details (see `reports::model`).
pub fn report_details<C>(d: &str, _: &C) -> Result {
    if d.chars().count() > REPORT_DETAILS_MAX {
        return Err(Error::new("details must be at most 2000 characters"));
    }
    Ok(())
}

/// Trimmed query (see [`de_query`]).
pub fn search_query<C>(q: &str, _: &C) -> Result {
    if q.chars().count() > QUERY_MAX {
        return Err(Error::new("query must be at most 200 characters"));
    }
    Ok(())
}

/// Request-side twin of `destination`'s `TIINY_BASE_URL` check: no link to the host that served
/// the request, so a link can't redirect to itself when the base URL is unset or another host is used.
pub fn not_this_host(raw: &str, this: Option<&str>) -> std::result::Result<(), AppError> {
    let to = url::Url::parse(raw).ok();
    let to = to
        .as_ref()
        .and_then(|u| u.host_str())
        .map(|h| h.trim_end_matches('.'));
    if this.is_some() && this == to {
        return Err(AppError::Unprocessable(
            "url must not point at this service".into(),
        ));
    }
    Ok(())
}

pub fn destination(raw: &str, config: &Config) -> Result {
    let bad = Error::new;
    if raw.len() > URL_MAX {
        return Err(bad("url is too long"));
    }
    if !raw.contains("://") {
        return Err(bad("url must start with http:// or https://"));
    }
    let url = url::Url::parse(raw).map_err(|_| bad("invalid url"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(bad("url must be http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("url must not contain credentials"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| bad("invalid url"))?
        .trim_end_matches('.');
    if config
        .base_url
        .as_ref()
        .and_then(|u| u.host_str())
        .is_some_and(|h| h.trim_end_matches('.') == host)
    {
        return Err(bad("url must not point at this service"));
    }
    if config
        .blocked_domains
        .iter()
        .any(|d| host == d || host.ends_with(&format!(".{d}")))
    {
        return Err(bad("domain is not allowed"));
    }
    Ok(())
}

/// Row id from a path or query string; anything below 1 is a 400 at extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "i64")]
pub struct Id(pub i64);

impl TryFrom<i64> for Id {
    type Error = &'static str;

    fn try_from(v: i64) -> std::result::Result<Self, Self::Error> {
        if v >= 1 {
            Ok(Self(v))
        } else {
            Err("id must be a positive integer")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(
            parse_email("  Foo@Example.COM ").unwrap(),
            "foo@example.com"
        );
        let local64 = format!("{}@x.com", "a".repeat(64));
        let max = format!(
            "a@{}.{}.{}.com",
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(120)
        );
        assert_eq!(max.len(), 254);
        for ok in [
            "a@b.co",
            "first.last+tag@sub.example.org",
            "a@xn--bcher-kva.example",
            "a@1-2.io",
            &local64,
            &max,
        ] {
            assert!(parse_email(ok).is_ok(), "{ok}");
        }
        let local65 = format!("{}@x.com", "a".repeat(65));
        let local66_bytes = format!("{}@x.com", "é".repeat(33));
        assert!(parse_email(&format!("{}@x.com", "é".repeat(32))).is_ok());
        let too_long = format!("a{max}");
        for bad in [
            "",
            "plain",
            "a@b",
            "@b.com",
            "a@@b.com",
            "a@b@c.com",
            "a b@c.com",
            "a\u{0}@c.com",
            "a@b c.com",
            "a@.b.com",
            "a@b.com.",
            "a@b..com",
            "a@-b.com",
            "a@b-.com",
            "a@b_c.com",
            "a@bü.com",
            &local65,
            &local66_bytes,
            &too_long,
        ] {
            assert_eq!(
                parse_email(bad).unwrap_err().message(),
                "invalid email address",
                "{bad}"
            );
        }
    }

    #[test]
    fn passwords() {
        let p = |s: &str| password(s, &());
        assert!(p("1234567").is_err());
        assert!(p("12345678").is_ok());
        assert!(p(&"é".repeat(128)).is_ok());
        assert!(p(&"x".repeat(129)).is_err());
        assert!(p("        ").is_err());
        assert!(p(" \t\n      ").is_err());
        assert!(p("   a    ").is_ok());
    }

    fn parse_name(raw: Option<&str>) -> std::result::Result<String, Error> {
        let n = normalize_name(raw);
        name(&n, &()).map(|()| n)
    }

    #[test]
    fn names() {
        for ok in [
            "José",
            "Zoë",
            "Ngô Bảo Châu",
            "O'Brien",
            "O’Brien",
            "D’Angelo",
            "Jean-Luc",
            "A. Smith",
            "e\u{301}mile",
            "李小龍",
            &"a".repeat(100),
        ] {
            assert_eq!(parse_name(Some(ok)).unwrap(), ok);
        }
        assert_eq!(
            parse_name(Some("  Ada \t\n Lovelace ")).unwrap(),
            "Ada Lovelace"
        );
        assert_eq!(
            parse_name(Some("Ada\u{85}Lovelace")).unwrap(),
            "Ada Lovelace"
        );
        assert_eq!(
            parse_name(Some(&format!("{} b", "a".repeat(98))))
                .unwrap()
                .len(),
            100
        );
        for empty in [None, Some(""), Some(" \u{85}\u{3000} ")] {
            assert!(
                parse_name(empty).is_err_and(|e| e.message() == "name is required"),
                "{empty:?}"
            );
        }
        for bad in [
            "dev123",
            "😀",
            "",
            " ",
            "-Bob",
            "'Bob",
            ".Bob",
            "\u{301}e",
            "Bob!",
            "Bob_Smith",
            "Ada\u{FEFF}",
            "\u{FEFF}Ada",
            &"a".repeat(101),
        ] {
            assert!(parse_name(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn tokens() {
        let good = URL_SAFE_NO_PAD.encode([0xab; SECRET_BYTES]);
        assert_eq!(good.len(), 43);
        assert!(token(&good).is_ok());
        assert!(token(&"A".repeat(43)).is_ok());
        for bad in [
            String::new(),
            "A".repeat(42),
            "A".repeat(44),
            format!("{}B", "A".repeat(42)),
            format!("{good}="),
            good.replacen(|_| true, "+", 1),
            good.replacen(|_| true, "/", 1),
            good.replacen(|_| true, ".", 1),
        ] {
            assert!(matches!(token(&bad), Err(AppError::BadRequest(_))), "{bad}");
        }
    }

    #[test]
    fn search_queries() {
        #[derive(Deserialize)]
        struct Q {
            #[serde(default, deserialize_with = "de_query")]
            query: Option<String>,
        }
        let q = |v: serde_json::Value| serde_json::from_value::<Q>(v).unwrap().query;
        assert_eq!(q(serde_json::json!({})), None);
        assert_eq!(q(serde_json::json!({ "query": "   " })), None);
        assert_eq!(
            q(serde_json::json!({ "query": " abc " })).as_deref(),
            Some("abc")
        );
        assert!(search_query(&"é".repeat(200), &()).is_ok());
        assert_eq!(
            search_query(&"q".repeat(201), &()).unwrap_err().message(),
            "query must be at most 200 characters"
        );
    }

    #[test]
    fn ids() {
        assert_eq!(Id::try_from(1), Ok(Id(1)));
        assert_eq!(Id::try_from(i64::MAX), Ok(Id(i64::MAX)));
        assert!(Id::try_from(0).is_err());
        assert!(Id::try_from(-1).is_err());
    }

    fn cfg(blocked: &str) -> Config {
        Config::from_vars([
            (
                "TIINY_BASE_URL".to_string(),
                "https://tiiny.test".to_string(),
            ),
            ("TIINY_BLOCKED_DOMAINS".to_string(), blocked.to_string()),
        ])
        .unwrap()
    }

    #[test]
    fn destinations_rejected() {
        let c = cfg("evil.com");
        let long = format!("https://x.com/{}", "a".repeat(URL_MAX - 13));
        assert_eq!(long.len(), URL_MAX + 1);
        for bad in [
            "ftp://x.com",
            "https://tiiny.test/abc",
            "https://TIINY.test:9/abc",
            "https://sub.evil.com",
            "https://evil.com",
            "https://evil.com.",
            "https://EVIL.com.",
            "https://tiiny.test.",
            "http://user@x.com",
            "not a url",
            "http://",
            &long,
        ] {
            assert!(destination(bad, &c).is_err(), "{bad}");
        }
    }

    #[test]
    fn destination_requires_scheme() {
        let c = cfg("evil.com");
        for bad in [
            "example.com/abc",
            "www.example.com",
            "not a url",
            "localhost:8080/x",
            "example.com:80/a",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            let e = destination(bad, &c).unwrap_err();
            assert_eq!(
                e.to_string(),
                "url must start with http:// or https://",
                "{bad}"
            );
        }
        assert!(destination("http://intranet/x", &c).is_ok());
        assert!(destination("http://192.168.1.5/", &c).is_ok());
    }

    #[test]
    fn destinations_accepted() {
        let c = cfg("evil.com");
        assert!(destination("HTTP://Example.com/path?q=1", &c).is_ok());
        assert!(destination("https://notevil.com", &c).is_ok());
        let max = format!("https://x.com/{}", "a".repeat(URL_MAX - 14));
        assert_eq!(max.len(), URL_MAX);
        assert!(destination(&max, &c).is_ok());
    }
}
