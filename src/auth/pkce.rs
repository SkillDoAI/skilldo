//! PKCE (Proof Key for Code Exchange) generation for OAuth 2.0.
//!
//! Generates cryptographically random code verifiers and S256 challenges
//! per RFC 7636.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// Generate a PKCE code verifier and its S256 challenge.
///
/// Returns `(verifier, challenge)` where:
/// - `verifier`: 43-char base64url string (from 32 random bytes)
/// - `challenge`: base64url(SHA-256(verifier))
pub fn generate_pkce() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = challenge_from_verifier(&verifier);
    (verifier, challenge)
}

/// Compute S256 challenge from a verifier string.
fn challenge_from_verifier(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a random state parameter for CSRF protection.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 16];
    rand::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_pkce_produces_valid_lengths() {
        let (verifier, challenge) = generate_pkce();
        // 32 bytes → 43 base64url chars (no padding)
        assert_eq!(verifier.len(), 43);
        // SHA-256 → 32 bytes → 43 base64url chars (no padding)
        assert_eq!(challenge.len(), 43);
    }

    #[test]
    fn generate_pkce_verifier_is_base64url() {
        let (verifier, _) = generate_pkce();
        assert!(verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn generate_pkce_challenge_matches_verifier() {
        let (verifier, challenge) = generate_pkce();
        assert_eq!(challenge_from_verifier(&verifier), challenge);
    }

    #[test]
    fn challenge_from_verifier_deterministic() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = challenge_from_verifier(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generate_pkce_produces_unique_values() {
        let (v1, _) = generate_pkce();
        let (v2, _) = generate_pkce();
        assert_ne!(v1, v2);
    }

    #[test]
    fn generate_state_produces_32_hex_chars() {
        let state = generate_state();
        assert_eq!(state.len(), 32);
        assert!(state.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_state_produces_unique_values() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
    }
}
