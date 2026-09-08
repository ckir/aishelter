use aws_rds_signer::Signer;
use lambda_runtime::{Error, LambdaEvent, run, service_fn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Column, Row, postgres::PgPoolOptions};
use tracing::info;

#[derive(Deserialize)]
struct QueryRequest {
    sql: String,
    #[serde(default)]
    params: Vec<Value>,
}

#[derive(Serialize)]
struct QueryResponse {
    rows: Vec<Value>,
    row_count: usize,
}

#[derive(Clone)]
struct AppState {
    database_url: String,
    iam_auth: bool,
    host: String,
    port: u16,
    user: String,
}

impl AppState {
    fn from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://pgadmin:password@localhost:5432/pgproxy".to_string());
        let iam_auth = std::env::var("RDS_IAM_AUTH").map(|v| v == "true").unwrap_or(false);

        // Parse host/port/user from URL for IAM token generation
        let url = url::Url::parse(&database_url).unwrap_or_else(|_| {
            url::Url::parse("postgresql://postgres@localhost:5432/postgres").unwrap()
        });
        let host = url.host_str().unwrap_or("localhost").to_string();
        let port = url.port().unwrap_or(5432);
        let user = url.username().to_string();

        Self { database_url, iam_auth, host, port, user }
    }

    async fn get_pool(&self) -> Result<sqlx::PgPool, Error> {
        let db_url = if self.iam_auth {
            info!("Generating RDS IAM auth token");
            let signer = Signer::builder()
                .host(self.host.clone())
                .port(self.port)
                .user(self.user.clone())
                .region("us-east-1".to_string())
                .build();

            let token = signer.fetch_token().await.expect("Failed to generate auth token");

            // Replace password in URL with IAM token
            if let Ok(mut url) = url::Url::parse(&self.database_url) {
                url.set_password(Some(&token)).ok();
                url.to_string()
            } else {
                self.database_url.clone()
            }
        } else {
            self.database_url.clone()
        };

        info!("Creating connection pool");
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await
            .map_err(|e| format!("Failed to connect to database: {}", e))?;

        Ok(pool)
    }
}

async fn handler(event: LambdaEvent<Value>, state: &AppState) -> Result<Value, Error> {
    let path = event.payload.get("rawPath").and_then(|v| v.as_str()).unwrap_or("/");
    let method = event
        .payload
        .get("requestContext")
        .and_then(|rc| rc.get("http"))
        .and_then(|http| http.get("method"))
        .and_then(|m| m.as_str())
        .unwrap_or("GET");

    match (method, path) {
        ("GET", "/health") => Ok(json!({"status": "ok", "service": "pg-proxy-lambda"})),
        ("POST", "/query") => handle_query(&event.payload, state).await,
        _ => Ok(json!({"error": "Not found. Use POST /query or GET /health"})),
    }
}

async fn handle_query(payload: &Value, state: &AppState) -> Result<Value, Error> {
    let body_str = payload.get("body").and_then(|b| b.as_str()).unwrap_or("{}");

    let query_req: QueryRequest = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(e) => return Ok(json!({"error": format!("Invalid JSON: {}", e)})),
    };

    info!("Executing query: {}", query_req.sql);

    let pool = match state.get_pool().await {
        Ok(p) => p,
        Err(e) => return Ok(json!({"error": format!("Database connection failed: {}", e)})),
    };

    // Build query with params
    let mut query = sqlx::query(&query_req.sql);
    for param in &query_req.params {
        query = query.bind(param.to_string());
    }

    let rows = match query.fetch_all(&pool).await {
        Ok(rows) => rows,
        Err(e) => return Ok(json!({"error": format!("Query failed: {}", e)})),
    };

    let rows: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for column in row.columns() {
                let value: Value = row.try_get(column.ordinal()).unwrap_or(Value::Null);
                map.insert(column.name().to_string(), value);
            }
            Value::Object(map)
        })
        .collect();

    let row_count = rows.len();
    Ok(json!(QueryResponse { rows, row_count }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("pg_proxy_lambda=info".parse().unwrap()),
        )
        .with_ansi(false)
        .init();

    info!("Starting pg-proxy-lambda");
    let state = AppState::from_env();

    let handler_fn = service_fn(move |event: LambdaEvent<Value>| {
        let state = state.clone();
        async move { handler(event, &state).await }
    });

    run(handler_fn).await
}
