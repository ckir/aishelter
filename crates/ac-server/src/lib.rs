pub mod config;
pub mod routes;
/// Axum HTTP server + routing.
pub mod server;

pub use server::create_app;
