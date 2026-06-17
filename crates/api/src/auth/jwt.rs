//! Free-tier **session JWT** (HS256) for the paid-API access layer (task 0277).
//!
//! Minted by `/auth/session` after a successful Turnstile check; verified by the
//! auth gate ([`super::require_auth`]) on every subsequent request. Short-lived
//! (the replay window is bounded; the SPA re-solves Turnstile to refresh).

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// Session lifetime in seconds (15 min).
pub const SESSION_TTL_SECS: u64 = 15 * 60;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    /// Issued-at (unix seconds).
    pub iat: u64,
    /// Expiry (unix seconds) — validated by `jsonwebtoken`.
    pub exp: u64,
    /// Tier marker. Only `"free"` is minted here; paid clients authenticate with
    /// `X-API-Key`, never a JWT.
    pub tier: String,
}

/// Mint a free-tier session token valid for [`SESSION_TTL_SECS`] from `now_secs`.
pub fn issue(secret: &str, now_secs: u64) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = SessionClaims {
        iat: now_secs,
        exp: now_secs + SESSION_TTL_SECS,
        tier: "free".to_string(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// True iff `token` is a valid, unexpired HS256 session token signed by `secret`.
/// Any error (bad signature, expired, malformed) → false (fail-closed).
pub fn verify(secret: &str, token: &str) -> bool {
    let validation = Validation::new(Algorithm::HS256);
    decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn issued_token_verifies() {
        let t = issue("sekret", now()).unwrap();
        assert!(verify("sekret", &t));
    }

    #[test]
    fn wrong_secret_rejected() {
        let t = issue("sekret", now()).unwrap();
        assert!(!verify("inny", &t));
    }

    #[test]
    fn expired_token_rejected() {
        // issued 1h ago → exp 45min ago.
        let t = issue("sekret", now() - 3600).unwrap();
        assert!(!verify("sekret", &t));
    }

    #[test]
    fn garbage_rejected() {
        assert!(!verify("sekret", "not.a.jwt"));
    }
}
