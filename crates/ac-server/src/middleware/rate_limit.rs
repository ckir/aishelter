//! Per-agent and global rate limiting middleware.

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

/// Simple token bucket for rate limiting.
///
/// Tokens refill continuously at a fixed rate up to `max_tokens`.
/// Each consumed request decrements the bucket; when empty, requests
/// are denied until enough tokens have refilled.
struct TokenBucket {
    /// Current number of available tokens.
    tokens: u64,
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
            tokens: max_tokens,
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
        self.tokens =
            (self.tokens as f64 + elapsed * self.refill_rate).min(self.max_tokens as f64) as u64;
        self.last_refill = now;
        if self.tokens > 0 {
            self.tokens -= 1;
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
    /// `agent_limit` caps requests from any single agent identified by
    /// the `X-Agent-Id` header. Both use the same `window_secs` for refill timing.
    pub fn new(global_limit: u64, agent_limit: u64, window_secs: u64) -> Self {
        Self {
            global: Arc::new(Mutex::new(TokenBucket::new(global_limit, window_secs))),
            per_agent: Arc::new(Mutex::new(HashMap::new())),
            agent_limit,
            window_secs,
        }
    }

    /// Check whether a request should be allowed.
    ///
    /// The global limit is always checked first. If an `agent_id` is provided,
    /// the per-agent limit is also checked. Returns `false` if either bucket
    /// is exhausted.
    pub fn check(&self, agent_id: Option<&str>) -> bool {
        // Check global limit first
        if !self.global.lock().unwrap().try_consume() {
            return false;
        }
        // Then per-agent limit
        if let Some(id) = agent_id {
            let mut map = self.per_agent.lock().unwrap();
            let bucket = map
                .entry(id.to_string())
                .or_insert_with(|| TokenBucket::new(self.agent_limit, self.window_secs));
            if !bucket.try_consume() {
                return false;
            }
        }
        true
    }
}

/// Extract the agent ID from the `X-Agent-Id` request header.
///
/// Returns `None` when the header is absent or contains non-UTF-8 bytes.
fn extract_agent_id(req: &Request<Body>) -> Option<String> {
    req.headers().get("X-Agent-Id").and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

/// Axum middleware for rate limiting.
///
/// Rejects requests with HTTP 429 (Too Many Requests) when the global
/// or per-agent token bucket is exhausted.  A `Retry-After` header is
/// included with the configured window in seconds.
pub async fn rate_limit(
    Extension(rate_limiter): Extension<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let agent_id = extract_agent_id(&req);
    if !rate_limiter.check(agent_id.as_deref()) {
        let mut resp = Response::new(Body::from("rate limit exceeded"));
        *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        resp.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&rate_limiter.window_secs.to_string()).unwrap(),
        );
        return resp;
    }
    next.run(req).await
}
