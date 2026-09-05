use sqlx::PgPool;

pub struct DiscoveryService {
    pool: PgPool,
}

impl DiscoveryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
