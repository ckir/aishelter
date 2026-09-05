/// Axum HTTP server + routing.
pub mod server;
pub mod config;

pub use server::create_app;
