use bytes::BytesMut;
use lambda_runtime::{Error, LambdaEvent, run, service_fn};
use postgres::{
    Client, NoTls, Row,
    types::{IsNull, ToSql, Type},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::error::Error as StdError;
use tracing::info;

#[derive(Debug)]
enum SqlValue {
    Text(Option<String>),
    Bool(bool),
    Int(i64),
    Float(f64),
    Json(Value),
}

impl ToSql for SqlValue {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn StdError + Sync + Send>> {
        match self {
            SqlValue::Text(Some(s)) => <String as ToSql>::to_sql(s, ty, out),
            SqlValue::Text(None) => Ok(IsNull::Yes),
            SqlValue::Bool(b) => <bool as ToSql>::to_sql(b, ty, out),
            SqlValue::Int(i) => <i64 as ToSql>::to_sql(i, ty, out),
            SqlValue::Float(f) => <f64 as ToSql>::to_sql(f, ty, out),
            SqlValue::Json(v) => <Value as ToSql>::to_sql(v, ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        String::accepts(ty)
            || bool::accepts(ty)
            || i64::accepts(ty)
            || f64::accepts(ty)
            || Value::accepts(ty)
    }

    fn to_sql_checked(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn StdError + Sync + Send>> {
        self.to_sql(ty, out)
    }
}

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

        // Ensure sslmode=require for Lambda (security best practice)
        let conn_str = if database_url.contains("sslmode") {
            database_url
        } else {
            format!("{}?sslmode=require", database_url)
        };

        Self { conn_str }
    }

    fn connect(&self) -> Result<Client, postgres::Error> {
        info!("Connecting to RDS");
        Client::connect(&self.conn_str, NoTls)
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

fn handler(event: LambdaEvent<Value>, state: &AppState) -> Result<Value, Error> {
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
        ("POST", "/query") => handle_query(&event.payload, state),
        _ => Ok(json!({"error": "Not found. Use POST /query or GET /health"})),
    }
}

fn handle_query(payload: &Value, state: &AppState) -> Result<Value, Error> {
    let body_str = payload.get("body").and_then(|b| b.as_str()).unwrap_or("{}");

    let query_req: QueryRequest = match serde_json::from_str(body_str) {
        Ok(req) => req,
        Err(e) => return Ok(json!({"error": format!("Invalid JSON: {}", e)})),
    };

    info!("Executing query: {}", query_req.sql);

    let mut client = match state.connect() {
        Ok(c) => c,
        Err(e) => return Ok(json!({"error": format!("Database connection failed: {}", e)})),
    };

    // Build params as a slice of &dyn ToSql + Sync
    let to_sql_values: Vec<SqlValue> = query_req
        .params
        .iter()
        .map(|v| match v {
            Value::Null => SqlValue::Text(None),
            Value::Bool(b) => SqlValue::Bool(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    SqlValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    SqlValue::Float(f)
                } else {
                    SqlValue::Text(Some(n.to_string()))
                }
            }
            Value::String(s) => SqlValue::Text(Some(s.clone())),
            Value::Array(_) | Value::Object(_) => SqlValue::Json(v.clone()),
        })
        .collect();

    let param_refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
        to_sql_values.iter().map(|v| v as &(dyn postgres::types::ToSql + Sync)).collect();

    let rows = match client.query(&query_req.sql, &param_refs) {
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
        std::future::ready(handler(event, &state))
    });

    run(handler_fn).await
}
