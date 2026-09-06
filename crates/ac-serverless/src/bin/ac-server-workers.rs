//! Cloudflare Workers serverless binary.
//!
//! Workers doesn't have a traditional main function. The worker entry
//! point is the `#[event(fetch)]` attribute in the Workers runtime.
//! This binary provides a stub that compiles cleanly for workspace checks.
//!
//! For actual Workers deployment, use `worker-build` to compile to wasm
//! and deploy with `wrangler deploy`.

use ac_runtime::adapters::workers::WorkersHttpAdapter;

fn main() {
    // Workers deployment uses wasm target, not native binary.
    // This stub exists so `cargo check` passes for the workspace.
    let _adapter = WorkersHttpAdapter::new();
    tracing::info!("Workers adapter constructs (native target stub)");
}
