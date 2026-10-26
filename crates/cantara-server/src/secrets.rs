//! Tokens, passwords and the hashes they are kept as.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// A new random token: 32 bytes, URL-safe base64. Long enough that guessing
/// one is not a strategy.
pub fn random_token() -> AppResult<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(AppError::internal)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// What a token is stored as. SHA-256 is enough here — unlike a password, a
/// token is random and long, so there is nothing to slow an attacker down on.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(AppError::internal)?;
    let salt = SaltString::encode_b64(&salt).map_err(AppError::internal)?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(AppError::internal)
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|hash| Argon2::default().verify_password(password.as_bytes(), &hash).is_ok())
        .unwrap_or(false)
}

/// Whether `verifier` is the one `challenge` was made from (PKCE, `S256`).
pub fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
}

/// The rules a new password has to meet. Deliberately few: length is what
/// matters, and rules about digits and symbols make passwords worse.
pub fn check_password_rules(password: &str) -> AppResult<()> {
    if password.chars().count() < 10 {
        return Err(AppError::BadRequest(
            "the password needs at least 10 characters".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_verifies_against_its_own_hash_only() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong horse battery", &hash));
        assert!(!verify_password("anything", "not a hash"));
    }

    #[test]
    fn pkce_follows_rfc_7636() {
        // The example of RFC 7636, appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(pkce_matches(verifier, challenge));
        assert!(!pkce_matches("something else", challenge));
    }

    #[test]
    fn tokens_are_unique_and_hash_the_same_each_time() {
        let one = random_token().unwrap();
        let two = random_token().unwrap();
        assert_ne!(one, two);
        assert_eq!(hash_token(&one), hash_token(&one));
    }
}
