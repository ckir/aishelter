//! Manifest signing and verification using Ed25519 signatures.

use ac_crypto::AgentKeypair;
use ac_crypto::public_key_from_hex;
use ed25519_dalek::{Signature, Signer, Verifier};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during manifest signing or verification.
#[derive(Debug, Error)]
pub enum ManifestSignatureError {
    /// The signature could not be generated.
    #[error("failed to sign manifest: {0}")]
    SignError(String),

    /// The signature verification failed.
    #[error("signature verification failed: {0}")]
    VerifyError(String),

    /// The manifest JSON could not be serialized.
    #[error("failed to serialize manifest: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// A signed manifest wrapper that includes the raw manifest and its
/// Ed25519 signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest<T> {
    /// The raw manifest payload.
    pub manifest: T,
    /// Hex-encoded Ed25519 signature of the canonical JSON.
    pub signature: String,
    /// The public key (hex) of the signer.
    pub signer_public_key: String,
}

/// Canonicalize a serializable value to a JSON string for signing.
///
/// The canonicalization produces a compact JSON string with no
/// insignificant whitespace so that the same logical document always
/// produces the same byte sequence.
fn canonicalize<T: Serialize>(value: &T) -> Result<String, ManifestSignatureError> {
    let json = serde_json::to_string(value)?;
    Ok(json)
}

/// Sign a manifest with the given Ed25519 keypair.
///
/// # Arguments
///
/// * `manifest` — The manifest to sign (must implement `Serialize`).
/// * `keypair` — The Ed25519 signing key.
///
/// # Returns
///
/// A hex-encoded signature string.
///
/// # Errors
///
/// Returns [`ManifestSignatureError`] if serialization or signing fails.
pub fn sign_manifest<T: Serialize>(
    manifest: &T,
    keypair: &AgentKeypair,
) -> Result<String, ManifestSignatureError> {
    let canonical = canonicalize(manifest)?;
    let sig = keypair.signing_key().sign(canonical.as_bytes());
    Ok(hex::encode(sig.to_bytes()))
}

/// Verify a manifest signature against a known public key.
///
/// # Arguments
///
/// * `manifest` — The manifest whose signature should be verified.
/// * `signature` — Hex-encoded Ed25519 signature.
/// * `public_key_hex` — Hex-encoded Ed25519 public key of the expected signer.
///
/// # Errors
///
/// Returns [`ManifestSignatureError`] if verification fails.
pub fn verify_manifest_signature<T: Serialize>(
    manifest: &T,
    signature: &str,
    public_key_hex: &str,
) -> Result<(), ManifestSignatureError> {
    let canonical = canonicalize(manifest)?;

    let verifying_key = public_key_from_hex(public_key_hex)
        .map_err(|e| ManifestSignatureError::VerifyError(e.to_string()))?;

    let sig_bytes =
        hex::decode(signature).map_err(|e| ManifestSignatureError::VerifyError(e.to_string()))?;

    let sig = Signature::try_from(sig_bytes.as_slice())
        .map_err(|e| ManifestSignatureError::VerifyError(e.to_string()))?;

    verifying_key
        .verify(canonical.as_bytes(), &sig)
        .map_err(|e| ManifestSignatureError::VerifyError(e.to_string()))
}
