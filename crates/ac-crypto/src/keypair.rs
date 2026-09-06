//! Ed25519 keypair management.
//!
//! This module provides [`AgentKeypair`] for generating and managing an
//! agent's Ed25519 signing key, and [`public_key_from_hex`] for deserializing
//! a peer's public key from its hex-encoded form.
//!
//! Each agent in the commons holds a single Ed25519 keypair (§10).  The
//! private key never leaves the agent's control; only the public key is
//! registered with the server.

use ed25519_dalek::{SignatureError, SigningKey, VerifyingKey};
use rand_core::OsRng;

/// A wrapper around an Ed25519 signing key.
///
/// This type owned the secret key material and provides methods to derive
/// the corresponding public key and sign arbitrary messages.  In production
/// deployments the key should be stored in a secure enclave or HSM.
#[derive(Debug)]
pub struct AgentKeypair {
    /// The underlying Ed25519 signing key.
    signing_key: SigningKey,
}

impl AgentKeypair {
    /// Generate a new random keypair using OS entropy.
    ///
    /// This is the standard way for a new agent to create its identity.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Return the public key as a hex-encoded string.
    ///
    /// The returned 64-character hex string is what gets stored in the
    /// `agents.public_key` column and used for signature verification (§10).
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Return a reference to the underlying signing key.
    ///
    /// Used internally by [`SignedRequest::sign`](crate::signature::SignedRequest::sign).
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }
}

/// Deserialize a public key from a hex-encoded string.
///
/// Takes the 64-character hex string produced by [`AgentKeypair::public_key_hex`]
/// and reconstructs the [`VerifyingKey`] used for signature verification.
///
/// # Errors
///
/// Returns [`SignatureError`] if the input is not valid hex or does not
/// decode to exactly 32 bytes.
pub fn public_key_from_hex(hex_key: &str) -> Result<VerifyingKey, SignatureError> {
    // Decode the hex string into raw bytes.
    let bytes = hex::decode(hex_key).map_err(|_| SignatureError::new())?;
    // Ensure we have exactly 32 bytes (Ed25519 public key size).
    let bytes_array: [u8; 32] = bytes.try_into().map_err(|_| SignatureError::new())?;
    // Reconstruct the verifying key from the raw bytes.
    VerifyingKey::from_bytes(&bytes_array)
}
