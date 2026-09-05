use ed25519_dalek::{Signer, Verifier, VerifyingKey, Signature, SignatureError};
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use crate::keypair::AgentKeypair;

/// A signed request from an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedRequest {
    pub agent_id: String,
    pub timestamp: String,
    pub nonce: String,
    pub method: String,
    pub path: String,
    pub body_sha256: String,
    pub signature: String,
}

impl SignedRequest {
    /// Compute SHA-256 hash of a body.
    pub fn hash_body(body: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body);
        format!("{:x}", hasher.finalize())
    }

    /// Sign a request with the given keypair.
    pub fn sign(
        keypair: &AgentKeypair,
        agent_id: &str,
        method: &str,
        path: &str,
        body: &[u8],
        nonce: &str,
    ) -> Self {
        let body_sha256 = Self::hash_body(body);
        let timestamp = chrono::Utc::now().to_rfc3339();

        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            agent_id, timestamp, nonce, method, path, body_sha256
        );

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

    /// Verify the signature against a public key.
    pub fn verify(&self, public_key: &VerifyingKey) -> Result<(), SignatureError> {
        let message = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            self.agent_id,
            self.timestamp,
            self.nonce,
            self.method,
            self.path,
            self.body_sha256
        );

        let sig_bytes = hex::decode(&self.signature).map_err(|_| SignatureError::new())?;
        let sig = Signature::try_from(sig_bytes.as_slice()).map_err(|_| SignatureError::new())?;
        public_key.verify(message.as_bytes(), &sig)
    }
}
