pub const RESERVED_SLUGS: &[&str] = &[
    "api",
    "admin",
    "dashboard",
    "login",
    "logout",
    "signup",
    "auth",
    "settings",
    "healthz",
    "_app",
    "static",
    "assets",
    "favicon.ico",
    "robots.txt",
    "features",
    "self-host",
    "privacy",
    "terms",
    "report",
];

pub fn random_base62(len: usize) -> String {
    use rand::distr::{Alphanumeric, SampleString};
    Alphanumeric.sample_string(&mut rand::rng(), len)
}

pub fn is_well_formed_slug(s: &str) -> bool {
    (3..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// garde rule for a user-chosen slug.
pub fn custom_slug<C>(slug: &str, _: &C) -> garde::Result {
    if !is_well_formed_slug(slug) {
        return Err(garde::Error::new(
            "slug must be 3-64 characters of letters, digits, '-' or '_'",
        ));
    }
    if RESERVED_SLUGS.contains(&slug) {
        return Err(garde::Error::new("slug is reserved"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn random_slugs() {
        let set: HashSet<String> = (0..1000).map(|_| random_base62(7)).collect();
        assert_eq!(set.len(), 1000);
        assert!(
            set.iter()
                .all(|s| s.len() == 7 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
        );
        let used: HashSet<char> = set.iter().flat_map(|s| s.chars()).collect();
        assert_eq!(used.len(), 62);
    }

    #[test]
    fn reserved_slugs_match_frontend_schema() {
        let ts = include_str!("../../web/src/lib/schemas.ts");
        let list = ts
            .split_once("const reservedSlugs = [")
            .and_then(|(_, rest)| rest.split_once(']'))
            .expect("reservedSlugs array in web/src/lib/schemas.ts")
            .0;
        let ts_slugs: Vec<&str> = list
            .split(',')
            .map(|s| s.trim().trim_matches('\''))
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(ts_slugs, RESERVED_SLUGS);
    }

    #[test]
    fn custom_slugs() {
        for ok in ["abc", "My-Link_1", &"a".repeat(64)] {
            assert!(custom_slug(ok, &()).is_ok(), "{ok}");
        }
        for bad in [
            "ab",
            &"a".repeat(65),
            "a b",
            "é",
            "a.b",
            "api",
            "dashboard",
            "healthz",
        ] {
            assert!(custom_slug(bad, &()).is_err(), "{bad}");
        }
        assert!(is_well_formed_slug("api"));
        assert!(!is_well_formed_slug("ab"));
    }
}
