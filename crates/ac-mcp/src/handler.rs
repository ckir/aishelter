//! MCP JSON-RPC 2.0 router for Agent Commons.
//!
//! Translates incoming MCP JSON-RPC requests into ACP/1 internal service
//! calls and maps responses back to JSON-RPC 2.0 format.  The single
//! `POST /mcp` endpoint accepts all MCP methods.

use ac_db::pool::SharedPool;
use ac_discovery::service::{DiscoveryService, SearchQuery};
use ac_mailbox::service::MailboxService;
use ac_registry::service::RegistryService;
use ac_tasks::service::TaskService;
use ac_validation::service::ValidationService;
use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::list_tools;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 envelope types
// ---------------------------------------------------------------------------

/// Incoming JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Method name (e.g. `"tools/list"`, `"tools/call"`).
    pub method: String,
    /// Optional method parameters.
    pub params: Option<Value>,
    /// Request identifier (null for notifications).
    pub id: Option<Value>,
}

/// Successful JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcSuccess<T> {
    pub jsonrpc: &'static str,
    pub result: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

/// JSON-RPC 2.0 error response.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub jsonrpc: &'static str,
    pub error: RpcErrorBody,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
}

/// Unified JSON-RPC 2.0 response (success or error).
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum JsonRpcResponse {
    Success(JsonRpcSuccess<Value>),
    Error(JsonRpcError),
}

impl From<JsonRpcSuccess<Value>> for JsonRpcResponse {
    fn from(s: JsonRpcSuccess<Value>) -> Self {
        JsonRpcResponse::Success(s)
    }
}

impl From<JsonRpcError> for JsonRpcResponse {
    fn from(e: JsonRpcError) -> Self {
        JsonRpcResponse::Error(e)
    }
}

/// The `error` object inside a JSON-RPC error response.
#[derive(Debug, Serialize)]
pub struct RpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// ---------------------------------------------------------------------------
// tools/call argument shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ToolsCallParams {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SearchAgentsArgs {
    capability: Option<String>,
    min_reliability: Option<f64>,
    protocol: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RegisterAgentArgs {
    agent_id: String,
    public_key: String,
    profile_name: Option<String>,
    profile_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendMessageArgs {
    from_agent_id: String,
    to_agent_id: String,
    message_type: String,
    payload: Value,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskArgs {
    requester: String,
    capability: String,
    description: String,
    input: Value,
    deadline: Option<String>,
    verification_method: Option<String>,
    required_validators: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct SubmitResultArgs {
    task_id: String,
    agent_id: String,
    result: Value,
    output_hash: String,
}

#[derive(Debug, Deserialize)]
struct ValidateArgs {
    task_id: String,
    validator_id: String,
    decision: String,
    reasoning: Option<String>,
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// JSON-RPC error codes used by this adapter.
mod codes {
    #[allow(dead_code)]
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    #[allow(dead_code)]
    pub const INTERNAL_ERROR: i64 = -32603;
    /// Application-level error (mapped from [`ac_types::error::AcError`]).
    pub const APPLICATION_ERROR: i64 = -32000;
}

fn rpc_error(code: i64, message: impl Into<String>, id: Option<Value>) -> Json<JsonRpcResponse> {
    Json(JsonRpcResponse::Error(JsonRpcError {
        jsonrpc: "2.0",
        error: RpcErrorBody { code, message: message.into(), data: None },
        id,
    }))
}

fn success(result: Value, id: Option<Value>) -> Json<JsonRpcResponse> {
    Json(JsonRpcResponse::Success(JsonRpcSuccess { jsonrpc: "2.0", result, id }))
}

// ---------------------------------------------------------------------------
// Tool call dispatch
// ---------------------------------------------------------------------------

/// Execute a named `tools/call` request against the ACP/1 service layer.
async fn dispatch_call(
    pool: &SharedPool,
    name: &str,
    arguments: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    match name {
        "search_agents" => call_search_agents(pool, arguments).await,
        "register_agent" => call_register_agent(pool, arguments).await,
        "send_message" => call_send_message(pool, arguments).await,
        "create_task" => call_create_task(pool, arguments).await,
        "submit_result" => call_submit_result(pool, arguments).await,
        "validate" => call_validate(pool, arguments).await,
        other => Err(ac_types::error::AcError::Internal(format!("unknown MCP tool: {other}"))),
    }
}

async fn call_search_agents(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: SearchAgentsArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad search_agents args: {e}")))?
        .unwrap_or(SearchAgentsArgs {
            capability: None,
            min_reliability: None,
            protocol: None,
            status: None,
            limit: None,
        });

    let db_pool = pool.load();
    let service = DiscoveryService::new(db_pool);
    let query = SearchQuery {
        capability: args.capability,
        min_reliability: args.min_reliability,
        protocol: args.protocol,
        status: args.status,
        limit: args.limit.unwrap_or(20),
    };

    let results = service.search_agents(&query).await?;
    Ok(serde_json::json!({
        "results": results,
        "count": results.len(),
        "protocol": "acp/1",
    }))
}

async fn call_register_agent(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: RegisterAgentArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad register_agent args: {e}")))?
        .ok_or_else(|| {
            ac_types::error::AcError::Internal("register_agent requires arguments".to_string())
        })?;

    let db_pool = pool.load();
    let service = RegistryService::new(db_pool);
    let agent = service
        .register_agent(
            &args.agent_id,
            &args.public_key,
            args.profile_name,
            args.profile_description,
        )
        .await?;

    Ok(serde_json::json!({
        "agent_id": agent.agent_id,
        "status": agent.status,
        "protocol": "acp/1",
    }))
}

async fn call_send_message(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: SendMessageArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad send_message args: {e}")))?
        .ok_or_else(|| {
            ac_types::error::AcError::Internal("send_message requires arguments".to_string())
        })?;

    let expires_at = args
        .expires_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let db_pool = pool.load();
    let service = MailboxService::new(db_pool);
    let message_id = service
        .send_message(
            &args.from_agent_id,
            &args.to_agent_id,
            &args.message_type,
            args.payload,
            expires_at,
        )
        .await?;

    Ok(serde_json::json!({
        "status": "sent",
        "message_id": message_id,
    }))
}

async fn call_create_task(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: CreateTaskArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad create_task args: {e}")))?
        .ok_or_else(|| {
            ac_types::error::AcError::Internal("create_task requires arguments".to_string())
        })?;

    let deadline = args
        .deadline
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let verification_method = args.verification_method.unwrap_or_else(|| "peer".to_string());
    let required_validators = args.required_validators.unwrap_or(2);

    let db_pool = pool.load();
    let service = TaskService::new(db_pool);
    let task_id = service
        .create_task(
            &args.requester,
            &args.capability,
            &args.description,
            args.input,
            deadline,
            &verification_method,
            required_validators,
        )
        .await?;

    Ok(serde_json::json!({
        "task_id": task_id,
        "status": "CREATED",
    }))
}

async fn call_submit_result(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: SubmitResultArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad submit_result args: {e}")))?
        .ok_or_else(|| {
            ac_types::error::AcError::Internal("submit_result requires arguments".to_string())
        })?;

    let db_pool = pool.load();
    let service = TaskService::new(db_pool);
    service.submit_result(&args.task_id, &args.agent_id, args.result, &args.output_hash).await?;

    Ok(serde_json::json!({
        "status": "submitted",
    }))
}

async fn call_validate(
    pool: &SharedPool,
    args: Option<Value>,
) -> Result<Value, ac_types::error::AcError> {
    let args: ValidateArgs = args
        .map(|v| serde_json::from_value(v))
        .transpose()
        .map_err(|e| ac_types::error::AcError::Internal(format!("bad validate args: {e}")))?
        .ok_or_else(|| {
            ac_types::error::AcError::Internal("validate requires arguments".to_string())
        })?;

    let db_pool = pool.load();
    let service = ValidationService::new(db_pool);
    let decision = service
        .validate_task(&args.task_id, &args.validator_id, &args.decision, args.reasoning.as_deref())
        .await?;

    let decision_str = match decision {
        ac_validation::quorum::QuorumDecision::Verified => "verified",
        ac_validation::quorum::QuorumDecision::Rejected => "rejected",
        ac_validation::quorum::QuorumDecision::Disputed => "disputed",
        ac_validation::quorum::QuorumDecision::Pending => "pending",
    };

    Ok(serde_json::json!({
        "decision": decision_str,
    }))
}

// ---------------------------------------------------------------------------
// Main MCP handler
// ---------------------------------------------------------------------------

/// POST /mcp — JSON-RPC 2.0 endpoint.
///
/// Accepts MCP `tools/list` and `tools/call` methods and translates them
/// into ACP/1 service calls.  Unsupported methods return JSON-RPC
/// `Method not found`.
async fn mcp_handler(
    State(pool): State<SharedPool>,
    Json(req): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = req.id.clone();

    match req.method.as_str() {
        "tools/list" => {
            let tools: Vec<Value> = list_tools()
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();
            success(serde_json::json!({ "tools": tools }), id)
        }

        "tools/call" => {
            let params: ToolsCallParams = match req.params.map(|v| serde_json::from_value(v)) {
                Some(Ok(p)) => p,
                Some(Err(e)) => {
                    return rpc_error(codes::INVALID_REQUEST, e.to_string(), id);
                }
                None => {
                    return rpc_error(codes::INVALID_REQUEST, "tools/call requires params", id);
                }
            };

            match dispatch_call(&pool, &params.name, params.arguments).await {
                Ok(result) => success(
                    serde_json::json!({
                        "content": [{ "type": "text", "text": result.to_string() }],
                    }),
                    id,
                ),
                Err(e) => rpc_error(codes::APPLICATION_ERROR, e.to_string(), id),
            }
        }

        other => rpc_error(codes::METHOD_NOT_FOUND, format!("method not found: {other}"), id),
    }
}

/// Build the MCP JSON-RPC router.
///
/// The router exposes a single `POST /mcp` endpoint.  Callers send
/// JSON-RPC 2.0 envelopes and receive MCP-compatible responses.
pub fn mcp_router(pool: SharedPool) -> Router {
    Router::new().route("/mcp", post(mcp_handler)).with_state(pool)
}
