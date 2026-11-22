//! Global and per-agent rate limiting middleware.
//!
//! * [`rate_limit`] is the outermost guard. It applies the **global** bucket
//!   to every request and never looks at client-supplied identity.
//! * [`rate_limit_agent`] applies the **per-agent** bucket. It must run behind
//!   the authentication layer and keys on the verified [`AuthenticatedAgent`],
//!   so a caller cannot drain another agent's allowance by naming it in
//!   `X-Agent-Id`, and cannot mint fresh allowances by rotating fake ids.

use ac_types::auth::AuthenticatedAgent;
use axum::{
    body::Body,
    extract::Extension,
    http::{HeaderValue, Request, StatusCode, header},
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

/// Most per-agent buckets kept before idle ones are discarded.
pub const MAX_TRACKED_AGENTS: usize = 10_000;

/// Simple token bucket for rate limiting.
///
/// Tokens refill continuously at a fixed rate up to `max_tokens`.
/// Each consumed request decrements the bucket; when empty, requests
/// are denied until enough tokens have refilled.
struct TokenBucket {
    /// Current number of available tokens.
    tokens: f64,
    /// Maximum number of tokens the bucket can hold.
    max_tokens: u64,
    /// Time of the last token refill calculation.
    last_refill: Instant,
    /// Tokens added per second.
    refill_rate: f64,
}

impl TokenBucket {
    /// Create a new token bucket.
    ///
    /// `max_tokens` is both the initial token count and the capacity.
    /// `window_secs` defines the time window over which all tokens
    /// would be consumed at the refill rate.
    fn new(max_tokens: u64, window_secs: u64) -> Self {
        Self {
            tokens: max_tokens as f64,
            max_tokens,
            last_refill: Instant::now(),
            refill_rate: max_tokens as f64 / window_secs as f64,
        }
    }

    /// Attempt to consume one token, refilling based on elapsed time first.
    ///
    /// Returns `true` if a token was consumed, `false` if the bucket is empty.
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        // Keep fractional tokens: truncating them while still resetting the
        // clock would discard the partial refill on every call and starve a
        // bucket that is polled faster than one token's refill time.
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens as f64);
        self.last_refill = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Shared rate limiter state with global and per-agent token buckets.
///
/// Cloning a [`RateLimiter`] is cheap — it shares the underlying buckets
/// via `Arc<Mutex<_>>`. Each clone refers to the same rate-limit state.
#[derive(Clone)]
pub struct RateLimiter {
    /// Global token bucket shared across all agents.
    global: Arc<Mutex<TokenBucket>>,
    /// Per-agent token buckets keyed by agent ID.
    per_agent: Arc<Mutex<HashMap<String, TokenBucket>>>,
    /// Maximum tokens per agent bucket.
    agent_limit: u64,
    /// Refill window in seconds.
    window_secs: u64,
}

impl RateLimiter {
    /// Create a new rate limiter with separate global and per-agent limits.
    ///
    /// `global_limit` caps total requests across all agents.
    /// `agent_limit` caps requests from any single verified agent. Both use the same `window_secs` for refill timing.
    pub fn new(global_limit: u64, agent_limit: u64, window_secs: u64) -> Self {
        Self {
            global: Arc::new(Mutex::new(TokenBucket::new(global_limit, window_secs))),
            per_agent: Arc::new(Mutex::new(HashMap::new())),
            agent_limit,
            window_secs,
        }
    }

    /// Consume one token from the global bucket.
    ///
    /// Returns `false` when the global bucket is exhausted.
    pub fn check_global(&self) -> bool {
        self.global.lock().unwrap().try_consume()
    }

    /// Consume one token from `agent_id`'s bucket.
    ///
    /// `agent_id` must be a verified identity. Buckets that have been idle
    /// for a full window (so would be full again anyway) are dropped once
    /// [`MAX_TRACKED_AGENTS`] is exceeded, which bounds memory.
    pub fn check_agent(&self, agent_id: &str) -> bool {
        let mut map = self.per_agent.lock().unwrap();
        if map.len() >= MAX_TRACKED_AGENTS && !map.contains_key(agent_id) {
            let idle = std::time::Duration::from_secs(self.window_secs);
            map.retain(|_, bucket| bucket.last_refill.elapsed() < idle);
        }
        map.entry(agent_id.to_string())
            .or_insert_with(|| TokenBucket::new(self.agent_limit, self.window_secs))
            .try_consume()
    }

    fn too_many_requests(&self) -> Response {
        let mut resp = Response::new(Body::from("rate limit exceeded"));
        *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        resp.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&self.window_secs.to_string()).unwrap(),
        );
        resp
    }
}

/// Global rate limit (outermost layer).
///
/// Rejects with HTTP 429 and a `Retry-After` header (the window in seconds)
/// when the global bucket is exhausted. Identity is deliberately ignored here.
pub async fn rate_limit(
    Extension(rate_limiter): Extension<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !rate_limiter.check_global() {
        return rate_limiter.too_many_requests();
    }
    next.run(req).await
}

/// Per-agent rate limit, keyed on the verified caller.
///
/// Layer it **inside** `authenticate`. A request that reaches it without an
/// [`AuthenticatedAgent`] is a wiring bug, and fails closed with a 500.
pub async fn rate_limit_agent(
    Extension(rate_limiter): Extension<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let Some(AuthenticatedAgent(agent_id)) = req.extensions().get::<AuthenticatedAgent>() else {
        tracing::error!("rate_limit_agent ran without an authenticated agent");
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("internal error"))
            .unwrap();
    };
    if !rate_limiter.check_agent(agent_id) {
        return rate_limiter.too_many_requests();
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn idle_buckets_are_dropped_once_the_map_is_full() {
        let limiter = RateLimiter::new(100, 5, 60);
        {
            let mut map = limiter.per_agent.lock().unwrap();
            for i in 0..MAX_TRACKED_AGENTS {
                let mut bucket = TokenBucket::new(5, 60);
                bucket.last_refill = Instant::now() - Duration::from_secs(120);
                map.insert(format!("idle-{i}"), bucket);
            }
        }
        assert!(limiter.check_agent("newcomer"));
        let len = limiter.per_agent.lock().unwrap().len();
        assert_eq!(len, 1, "all idle buckets were pruned, only the newcomer remains");
    }

    #[test]
    fn active_buckets_survive_pruning() {
        let limiter = RateLimiter::new(100, 5, 60);
        {
            let mut map = limiter.per_agent.lock().unwrap();
            for i in 0..MAX_TRACKED_AGENTS {
                map.insert(format!("busy-{i}"), TokenBucket::new(5, 60));
            }
        }
        assert!(limiter.check_agent("newcomer"));
        let len = limiter.per_agent.lock().unwrap().len();
        assert_eq!(len, MAX_TRACKED_AGENTS + 1, "recently used buckets are kept");
    }

    /// Polling a drained bucket faster than one token's refill time must not
    /// discard the partial refill each time (it used to truncate to zero and
    /// reset the clock, starving the bucket for as long as the polling lasted).
    #[test]
    fn fractional_refill_accumulates_under_fast_polling() {
        let mut bucket = TokenBucket::new(1, 1); // 1 token, refills 1 per second
        assert!(bucket.try_consume());
        bucket.last_refill = Instant::now() - Duration::from_millis(600);
        assert!(!bucket.try_consume(), "0.6 of a token is not enough yet");
        bucket.last_refill = Instant::now() - Duration::from_millis(600);
        assert!(bucket.try_consume(), "0.6 + 0.6 = 1.2 tokens: one request is allowed");
    }

    #[test]
    fn a_bucket_denies_after_its_allowance() {
        let limiter = RateLimiter::new(100, 2, 60);
        assert!(limiter.check_agent("a"));
        assert!(limiter.check_agent("a"));
        assert!(!limiter.check_agent("a"));
        assert!(limiter.check_agent("b"));
    }
}
