use chrono::TimeZone;
use hex::encode as hex_encode;
use hmac::{Hmac, Mac};
use lambda_runtime::{Error, LambdaEvent, run, service_fn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_postgres::{Client, NoTls, Row};
use tracing::info;
use url::Url;

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
    host: String,
    port: u16,
    dbname: String,
    user: String,
    password: String,
    iam_auth: bool,
}

impl AppState {
    fn from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://pgadmin:password@localhost:5432/pgproxy".to_string());
        let url = Url::parse(&database_url).expect("Invalid DATABASE_URL");
        let iam_auth = std::env::var("RDS_IAM_AUTH").map(|v| v == "true").unwrap_or(false);
        let password = url.password().unwrap_or("").to_string();

        Self {
            host: url.host_str().unwrap_or("localhost").to_string(),
            port: url.port().unwrap_or(5432),
            dbname: url.path().trim_start_matches('/').to_string(),
            user: url.username().to_string(),
            password,
            iam_auth,
        }
    }

    async fn connect(&self) -> Result<Client, tokio_postgres::Error> {
        info!("Connecting to RDS");
        let password = if self.iam_auth {
            info!("Generating RDS IAM auth token");
            generate_rds_token(&self.user, &self.host, self.port).unwrap_or_else(|e| {
                tracing::warn!("Failed to generate IAM token: {}", e);
                self.password.clone()
            })
        } else {
            self.password.clone()
        };

        let conn_str = format!(
            "host={} port={} dbname={} user={} password={} sslmode=require",
            self.host, self.port, self.dbname, self.user, password
        );

        let config = conn_str.parse::<tokio_postgres::Config>()?;
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

fn generate_rds_token(
    user: &str,
    host: &str,
    port: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simplified RDS IAM token generation
    // The password for PostgreSQL IAM auth is the full URL: https://host:port/?Action=connect&DBUser=user&...
    use std::env;
    let region = "us-east-1";
    let algorithm = "AWS4-HMAC-SHA256";
    let expires = "900";

    let now = SystemTime::now();
    let timestamp = now.duration_since(UNIX_EPOCH)?.as_secs();
    let date = chrono::Utc.timestamp_opt(timestamp as i64, 0).single().unwrap();
    let date_stamp = date.format("%Y%m%d").to_string();
    let amz_date = date.format("%Y%m%dT%H%M%SZ").to_string();

    let access_key = env::var("AWS_ACCESS_KEY_ID").map_err(|_| "No AWS_ACCESS_KEY_ID")?;
    let secret_key = env::var("AWS_SECRET_ACCESS_KEY").map_err(|_| "No AWS_SECRET_ACCESS_KEY")?;

    let credential = format!("{}/{}/{}/rds-db/aws4_request", access_key, date_stamp, region);

    // Build the canonical request
    let canonical_uri = "/";
    let canonical_query = format!(
        "Action=connect&DBUser={}&X-Amz-Algorithm={}&X-Amz-Credential={}&X-Amz-Date={}&X-Amz-Expires={}&X-Amz-SignedHeaders=host",
        user,
        algorithm,
        url_encode(&credential),
        amz_date,
        expires
    );

    let canonical_headers = format!("host:{}:{}\n", host, port);
    let signed_headers = "host";
    let payload_hash = hex_encode(Sha256::digest(b""));

    let canonical_request = format!(
        "GET\n{}\n{}\n{}\n{}\n{}",
        canonical_uri, canonical_query, canonical_headers, signed_headers, payload_hash
    );

    // String to sign
    let credential_scope = format!("{}/{}/rds-db/aws4_request", date_stamp, region);
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        algorithm,
        timestamp,
        credential_scope,
        hex_encode(Sha256::digest(canonical_request.as_bytes()))
    );

    // Sign
    let k_date = hmac_sha256(format!("AWS4{}", secret_key).as_bytes(), &date_stamp);
    let k_region = hmac_sha256(&k_date, region);
    let k_service = hmac_sha256(&k_region, "rds-db");
    let k_signing = hmac_sha256(&k_service, "aws4_request");
    let signature = hmac_sha256(&k_signing, &string_to_sign);

    // Build the full token URL
    let token = format!(
        "https://{}:{}/?{}&X-Amz-Signature={}",
        host,
        port,
        canonical_query,
        hex_encode(signature)
    );

    Ok(token)
}

fn url_encode(s: &str) -> String {
    s.replace('/', "%2F").replace('+', "%2B")
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC can take key");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
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
