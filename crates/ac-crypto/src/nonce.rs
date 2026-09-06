//! Nonce tracking for replay protection.
//!
//! Each signed request carries a unique nonce (§10).  The [`NonceStore`]
//! tracks which nonces have been seen and rejects any request whose nonce
//! has already been consumed, preventing attackers from replaying a
//! previously valid signed request.
//!
//! > **Note:** The current implementation uses an in-memory [`HashSet`].
//! > In production this must be backed by PostgreSQL with a TTL so that
//! > nonces expire after the request timestamp window elapses.

use std::collections::HashSet;

/// Tracks nonces to prevent replay attacks.
///
/// Each nonce is a unique opaque string included in the [`SignedRequest`](crate::signature::SignedRequest).
/// When the server processes a request, it calls [`check_and_insert`](NonceStore::check_and_insert);
/// if the nonce was already seen, the request is rejected as a replay.
///
/// # Production considerations
///
/// The current in-memory [`HashSet`] back-end is suitable for testing but
/// will lose state across restarts.  In production, use the `nonces`
/// PostgreSQL table with a background job that purges nonces older than
/// the timestamp tolerance window (e.g. 5 minutes).
pub struct NonceStore {
    /// Set of nonces that have been consumed.
    nonces: HashSet<String>,
}

impl NonceStore {
    /// Create an empty nonce store.
    pub fn new() -> Self {
        Self {
            nonces: HashSet::new(),
        }
    }

    /// Check whether a nonce has been used and record it if it is fresh.
    ///
    /// Returns `true` if this is a **new** nonce (the request should be
    /// processed) or `false` if the nonce was already consumed (the
    /// request should be rejected as a replay).
    pub fn check_and_insert(&mut self, nonce: &str) -> bool {
        // HashSet::insert returns true if the value was not already present.
        self.nonces.insert(nonce.to_string())
    }
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new()
    }
}
