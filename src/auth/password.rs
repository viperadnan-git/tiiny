use std::sync::LazyLock;

use argon2::{Argon2, PasswordHasher, PasswordVerifier};

use crate::error::AppError;

pub const MIN_LEN: usize = 8;
pub const MAX_LEN: usize = 128;

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    Argon2::default()
        .hash_password(pw.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    Argon2::default()
        .verify_password(pw.as_bytes(), hash)
        .is_ok()
}

/// [`hash_password`] off the async runtime.
pub async fn hash(pw: String) -> Result<String, AppError> {
    Ok(tokio::task::spawn_blocking(move || hash_password(&pw))
        .await
        .map_err(anyhow::Error::from)??)
}

/// [`verify_password`] off the async runtime; over-long input is rejected without hashing.
pub async fn verify(pw: String, hash: String) -> Result<bool, AppError> {
    if pw.chars().count() > MAX_LEN {
        return Ok(false);
    }
    Ok(
        tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
            .await
            .map_err(anyhow::Error::from)?,
    )
}

/// Verified against when the account has no usable hash, so failures take similar time.
pub static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash_password("dummy-password-for-timing").expect("argon2 hashing"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let h = hash_password("hunter22").unwrap();
        assert!(verify_password("hunter22", &h));
        assert!(!verify_password("hunter23", &h));
        assert!(!verify_password("hunter22", "not-a-hash"));
    }
}
