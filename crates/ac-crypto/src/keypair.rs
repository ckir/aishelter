use ed25519_dalek::{SigningKey, VerifyingKey, SignatureError};
use rand_core::OsRng;

/// A wrapper around an Ed25519 signing key.
#[derive(Debug)]
pub struct AgentKeypair {
    signing_key: SigningKey,
}

impl AgentKeypair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Return the public key as a hex-encoded string.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Return the signing key reference.
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }
}

/// Deserialize a public key from a hex string.
pub fn public_key_from_hex(hex_key: &str) -> Result<VerifyingKey, SignatureError> {
    let bytes = hex::decode(hex_key).map_err(|_| SignatureError::new())?;
    let bytes_array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SignatureError::new())?;
    VerifyingKey::from_bytes(&bytes_array)
}
