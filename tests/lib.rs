/// End-to-end integration tests for Agent Commons.
///
/// Tests the full demonstration scenarios from the spec:
/// - §63: First end-to-end demo (register → discover → task → validate → receipt)
/// - §64: Asynchronous mailbox (offline agent receives task)
/// - Adversarial: replay attacks, forgery, invalid transitions, spam
///
/// Run with: `DATABASE_URL=postgresql://... cargo test --test integration e2e -- --ignored`
mod e2e;
