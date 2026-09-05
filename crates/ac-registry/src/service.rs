use sqlx::PgPool;

/// Agent registration and Agent Card CRUD service.
pub struct RegistryService {
    pool: PgPool,
}

impl RegistryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
