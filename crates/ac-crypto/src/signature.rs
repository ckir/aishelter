//! Request signing and verification.
//!
//! This module defines [`SignedRequest`], which wraps an HTTP request with an
//! Ed25519 signature so that the server can authenticate the calling agent
//! and verify that the request body has not been tampered with (§11).
//!
//! The signing scheme produces a canonical string of the form
//! `agent_id\ntimestamp\nnonce\nmethod\npath\nbody_sha256` that is then
//! signed with the agent's Ed25519 private key.  The server reconstructs
//! the same string and verifies it against the agent's registered public key.

use crate::keypair::AgentKeypair;
use ed25519_dalek::{Signature, SignatureError, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// HTTP header names used for agent authentication.
pub const X_AGENT_ID: &str = "x-agent-id";
pub const X_AGENT_TIMESTAMP: &str = "x-agent-timestamp";
pub const X_AGENT_NONCE: &str = "x-agent-nonce";
pub const X_AGENT_SIGNATURE: &str = "x-agent-signature";

/// A signed HTTP request from an agent.
///
/// Every field is required for verification.  The signature covers the
/// agent identity, a timestamp, a nonce, the HTTP method and path, and
/// the SHA-256 hash of the request body (§11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedRequest {
    /// The agent ID that signed this request.
    pub agent_id: String,
    /// RFC 3339 timestamp when the request was signed.
    pub timestamp: String,
    /// Unique nonce to prevent replay attacks (§10).
    pub nonce: String,
    /// HTTP method (e.g. `"POST"`, `"GET"`).
    pub method: String,
    /// Request path (e.g. `"/v1/tasks"`).
    pub path: String,
    /// Hex-encoded SHA-256 hash of the request body.
    pub body_sha256: String,
    /// Hex-encoded Ed25519 signature over the canonical message string.
    pub signature: String,
}

impl SignedRequest {
    /// Compute the SHA-256 hash of a request body, returned as hex.
    ///
    /// This is used to bind the request body to the signature so that
    /// any modification of the payload invalidates the signature.
    pub fn hash_body(body: &[u8]) -> String {
        // Initialize the SHA-256 hasher.
        let mut hasher = Sha256::new();
        // Feed the body bytes into the hasher.
        hasher.update(body);
        // Finalize and return the hex digest.
        format!("{:x}", hasher.finalize())
    }

    /// Construct a [`SignedRequest`] by signing the canonical message string.
    ///
    /// # Arguments
    ///
    /// * `keypair` — the agent's Ed25519 keypair.
    /// * `agent_id` — the agent's unique identifier.
    /// * `method` — the HTTP method of the request.
    /// * `path` — the request path.
    /// * `body` — the raw request body bytes.
    /// * `nonce` — a unique nonce for replay protection.
    ///
    /// # Canonical message format
    ///
    /// The signed string is `agent_id\ntimestamp\nnonce\nmethod\npath\nbody_sha256`,
    /// where `timestamp` is the current UTC time in RFC 3339 format.
    pub fn sign(
        keypair: &AgentKeypair,
        agent_id: &str,
        method: &str,
        path: &str,
        body: &[u8],
        nonce: &str,
    ) -> Self {
        Self::sign_at(
            keypair,
            agent_id,
            method,
            path,
            body,
            nonce,
            &Self::format_timestamp(chrono::Utc::now()),
        )
    }

    /// Like [`sign`](Self::sign) but with an explicit timestamp string.
    ///
    /// The signature covers `timestamp` exactly as given, byte for byte.
    /// Production callers use [`sign`](Self::sign); this exists so tests can
    /// produce stale or future-dated requests deterministically.
    pub fn sign_at(
        keypair: &AgentKeypair,
        agent_id: &str,
        method: &str,
        path: &str,
        body: &[u8],
        nonce: &str,
        timestamp: &str,
    ) -> Self {
        let mut req = Self {
            agent_id: agent_id.to_string(),
            timestamp: timestamp.to_string(),
            nonce: nonce.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            body_sha256: Self::hash_body(body),
            signature: String::new(),
        };
        let sig = keypair.signing_key().sign(req.canonical_message().as_bytes());
        req.signature = hex::encode(sig.to_bytes());
        req
    }

    /// The canonical timestamp form clients should emit: RFC 3339, UTC, `Z`
    /// suffix, whole seconds (for example `2026-11-22T20:30:00Z`).
    pub fn format_timestamp(at: chrono::DateTime<chrono::Utc>) -> String {
        at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// Parse a client-supplied timestamp for the clock-skew check.
    ///
    /// Accepts any valid RFC 3339 string (any offset, optional fractional
    /// seconds) and rejects everything else, including bare Unix seconds.
    /// The signature is always computed over the string as received, never
    /// over a re-formatted value, so this is only used to read the instant.
    pub fn parse_timestamp(
        value: &str,
    ) -> Result<chrono::DateTime<chrono::Utc>, chrono::ParseError> {
        Ok(chrono::DateTime::parse_from_rfc3339(value)?.with_timezone(&chrono::Utc))
    }

    /// The exact byte string that is signed and verified.
    ///
    /// `path` is the request target including any query string, so a
    /// signature also binds the query parameters.
    fn canonical_message(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            self.agent_id, self.timestamp, self.nonce, self.method, self.path, self.body_sha256
        )
    }

    /// Verify the signature against the agent's public key.
    ///
    /// This reconstructs the canonical message string from the stored fields
    /// and verifies the Ed25519 signature.  The server calls this after
    /// looking up the agent's public key from the registry (§11).
    ///
    /// # Errors
    ///
    /// Returns [`SignatureError`] if the signature is malformed, the hex
    /// decoding fails, or the Ed25519 verification rejects.
    pub fn verify(&self, public_key: &VerifyingKey) -> Result<(), SignatureError> {
        // Reconstruct the canonical message string from stored fields.
        let message = self.canonical_message();

        // Decode the hex signature into raw bytes.
        let sig_bytes = hex::decode(&self.signature).map_err(|_| SignatureError::new())?;
        // Construct the Ed25519 Signature from the byte slice.
        let sig = Signature::try_from(sig_bytes.as_slice()).map_err(|_| SignatureError::new())?;
        // Verify the signature against the reconstructed message.
        public_key.verify(message.as_bytes(), &sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keypair::{AgentKeypair, public_key_from_hex};

    const AGENT: &str = "agent_test";

    fn signed(path: &str, body: &[u8]) -> (AgentKeypair, SignedRequest) {
        let kp = AgentKeypair::generate();
        let req = SignedRequest::sign(&kp, AGENT, "POST", path, body, "nonce-1");
        (kp, req)
    }

    #[test]
    fn sign_emits_utc_z_whole_second_timestamp() {
        let (_kp, req) = signed("/v1/messages", b"{}");
        // e.g. 2026-11-22T20:30:00Z : 20 chars, ends in Z, no fraction, no offset.
        assert_eq!(req.timestamp.len(), 20, "unexpected timestamp: {}", req.timestamp);
        assert!(req.timestamp.ends_with('Z'), "unexpected timestamp: {}", req.timestamp);
        assert!(!req.timestamp.contains('.'), "unexpected timestamp: {}", req.timestamp);
        assert!(SignedRequest::parse_timestamp(&req.timestamp).is_ok());
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let (kp, req) = signed("/v1/messages", b"{\"a\":1}");
        let pk = public_key_from_hex(&kp.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_ok());
    }

    #[test]
    fn sign_at_uses_the_given_timestamp_verbatim() {
        let kp = AgentKeypair::generate();
        let ts = "2026-01-02T03:04:05Z";
        let req = SignedRequest::sign_at(&kp, AGENT, "GET", "/v1/x", b"", "n", ts);
        assert_eq!(req.timestamp, ts);
        let pk = public_key_from_hex(&kp.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_ok());
    }

    #[test]
    fn verify_rejects_a_changed_query_string() {
        let kp = AgentKeypair::generate();
        let mut req = SignedRequest::sign(&kp, AGENT, "GET", "/v1/messages?agent_id=a", b"", "n");
        req.path = "/v1/messages?agent_id=b".to_string();
        let pk = public_key_from_hex(&kp.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_err());
    }

    #[test]
    fn verify_rejects_a_changed_body_hash() {
        let (kp, mut req) = signed("/v1/messages", b"original");
        req.body_sha256 = SignedRequest::hash_body(b"tampered");
        let pk = public_key_from_hex(&kp.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_err());
    }

    #[test]
    fn verify_rejects_a_changed_method() {
        let (kp, mut req) = signed("/v1/messages", b"x");
        req.method = "PUT".to_string();
        let pk = public_key_from_hex(&kp.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_err());
    }

    #[test]
    fn verify_rejects_another_agents_key() {
        let (_kp, req) = signed("/v1/messages", b"x");
        let other = AgentKeypair::generate();
        let pk = public_key_from_hex(&other.public_key_hex()).unwrap();
        assert!(req.verify(&pk).is_err());
    }

    #[test]
    fn parse_timestamp_accepts_z_offset_and_fraction() {
        let z = SignedRequest::parse_timestamp("2026-09-05T14:00:00Z").unwrap();
        let off = SignedRequest::parse_timestamp("2026-09-05T16:00:00+02:00").unwrap();
        let frac = SignedRequest::parse_timestamp("2026-09-05T14:00:00.250Z").unwrap();
        assert_eq!(z, off);
        assert_eq!(z.timestamp(), frac.timestamp());
    }

    #[test]
    fn parse_timestamp_rejects_unix_seconds_and_garbage() {
        for bad in ["1795375064", "", "yesterday", "2026-09-05", "2026-09-05 14:00:00"] {
            assert!(SignedRequest::parse_timestamp(bad).is_err(), "accepted {bad:?}");
        }
    }
}
