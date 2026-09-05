use sqlx::PgPool;

pub struct ReputationService {
    pool: PgPool,
}

impl ReputationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
