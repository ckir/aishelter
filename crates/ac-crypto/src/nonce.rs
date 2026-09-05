use std::collections::HashSet;

/// Tracks nonces to prevent replay attacks.
/// In production, this should be backed by PostgreSQL with TTL.
pub struct NonceStore {
    nonces: HashSet<String>,
}

impl NonceStore {
    pub fn new() -> Self {
        Self {
            nonces: HashSet::new(),
        }
    }

    /// Check if a nonce has been used. Returns true if this is a fresh nonce.
    pub fn check_and_insert(&mut self, nonce: &str) -> bool {
        self.nonces.insert(nonce.to_string())
    }
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new()
    }
}
