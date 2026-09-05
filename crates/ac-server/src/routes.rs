use axum::Router;

/// All API routes wired into the axum router.
pub fn routes() -> Router {
    Router::new()
        .merge(ac_registry::handler::routes())
        .merge(ac_discovery::handler::routes())
        .merge(ac_mailbox::handler::routes())
        .merge(ac_tasks::handler::routes())
        .merge(ac_validation::handler::routes())
        .merge(ac_reputation::handler::routes())
}
