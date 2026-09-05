use sqlx::PgPool;

pub struct ValidationService {
    pool: PgPool,
}

impl ValidationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
