/// End-to-end integration tests for Agent Commons.
///
/// Test the first demonstration scenario from §63 of the spec:
/// Agent A registers → publishes card → Agent B registers → A discovers B →
/// A creates task → B accepts → B submits → 2 validators verify →
/// receipt created → VWU + reputation event emitted.
mod e2e;
