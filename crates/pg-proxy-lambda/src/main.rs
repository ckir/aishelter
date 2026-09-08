use lambda_runtime::{Error, LambdaEvent, run, service_fn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_postgres::{Client, NoTls, Row};
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
    conn_str: String,
}

impl AppState {
    fn from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://pgadmin:password@localhost:5432/pgproxy".to_string());

        let conn_str = if database_url.contains("sslmode") {
            database_url
        } else {
            format!("{}?sslmode=require", database_url)
        };

        Self { conn_str }
    }

    async fn connect(&self) -> Result<Client, tokio_postgres::Error> {
        info!("Connecting to RDS");
        // Parse connection string into tokio-postgres config
        let config = self.conn_str.parse::<tokio_postgres::Config>()?;
        let (client, connection) = config.connect(NoTls).await?;

        // Spawn the connection driver
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::warn!("Connection error: {}", e);
            }
        });

        Ok(client)
    }
}

fn row_to_json(row: &Row) -> Value {
    let mut map = serde_json::Map::new();
    for (i, column) in row.columns().iter().enumerate() {
        let value: Value = row.try_get(i).unwrap_or(Value::Null);
        map.insert(column.name().to_string(), value);
    }
    Value::Object(map)
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

    let client = match state.connect().await {
        Ok(c) => c,
        Err(e) => return Ok(json!({"error": format!("Database connection failed: {}", e)})),
    };

    // Convert params to tokio-postgres types
    let params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = query_req
        .params
        .iter()
        .map(|v| -> Box<dyn tokio_postgres::types::ToSql + Sync> {
            match v {
                Value::Null => Box::new(None::<String>),
                Value::Bool(b) => Box::new(*b),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        Box::new(i)
                    } else if let Some(f) = n.as_f64() {
                        Box::new(f)
                    } else {
                        Box::new(n.to_string())
                    }
                }
                Value::String(s) => Box::new(s.clone()),
                Value::Array(_) | Value::Object(_) => Box::new(v.to_string()),
            }
        })
        .collect();

    let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        params.iter().map(|p| p.as_ref()).collect();

    let rows = match client.query(&query_req.sql, &param_refs).await {
        Ok(rows) => rows,
        Err(e) => return Ok(json!({"error": format!("Query failed: {}", e)})),
    };

    Ok(json!(QueryResponse { rows: rows.iter().map(row_to_json).collect(), row_count: rows.len() }))
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
