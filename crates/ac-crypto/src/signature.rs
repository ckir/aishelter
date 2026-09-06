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
        // Hash the request body.
        let body_sha256 = Self::hash_body(body);
        // Record the current timestamp.
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Build the canonical message string to sign.
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            agent_id, timestamp, nonce, method, path, body_sha256
        );

        // Sign the message with the agent's private key.
        let sig = keypair.signing_key().sign(message.as_bytes());

        Self {
            agent_id: agent_id.to_string(),
            timestamp,
            nonce: nonce.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            body_sha256,
            signature: hex::encode(sig.to_bytes()),
        }
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
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            self.agent_id, self.timestamp, self.nonce, self.method, self.path, self.body_sha256
        );

        // Decode the hex signature into raw bytes.
        let sig_bytes = hex::decode(&self.signature).map_err(|_| SignatureError::new())?;
        // Construct the Ed25519 Signature from the byte slice.
        let sig = Signature::try_from(sig_bytes.as_slice()).map_err(|_| SignatureError::new())?;
        // Verify the signature against the reconstructed message.
        public_key.verify(message.as_bytes(), &sig)
    }
}
