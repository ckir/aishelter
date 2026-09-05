use sqlx::PgPool;

pub struct MailboxService {
    pool: PgPool,
}

impl MailboxService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
