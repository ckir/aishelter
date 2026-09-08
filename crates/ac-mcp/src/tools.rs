//! MCP tool declarations for the Agent Commons adapter.
//!
//! Each tool exposes a name, description, and JSON Schema input definition.
//! These are returned by the `tools/list` MCP method and used by the
//! `tools/call` dispatcher to route to the corresponding ACP/1 operation.

use serde_json::{Value, json};

/// Metadata for a single MCP tool.
#[derive(Debug, Clone, serde::Serialize)]
pub struct McpTool {
    /// Tool name used in `tools/call` requests.
    pub name: &'static str,
    /// Human-readable description of what the tool does.
    pub description: &'static str,
    /// JSON Schema describing the `arguments` object for `tools/call`.
    pub input_schema: Value,
}

/// Return the complete list of available MCP tools.
///
/// The caller serialises this into the MCP `tools/list` response body.
pub fn list_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "search_agents",
            description: "Search for agents by capability, reliability, and protocol filters.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "capability": {
                        "type": "string",
                        "description": "Filter by capability name (e.g. \"fact_verification\")."
                    },
                    "min_reliability": {
                        "type": "number",
                        "description": "Minimum reliability score threshold (0.0..1.0)."
                    },
                    "protocol": {
                        "type": "string",
                        "description": "Protocol filter (e.g. \"acp/1\")."
                    },
                    "status": {
                        "type": "string",
                        "description": "Agent status filter (e.g. \"ACTIVE\")."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 20)."
                    }
                },
                "required": [],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "register_agent",
            description: "Register a new agent with its Ed25519 public key and optional profile.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "Globally unique agent identifier (e.g. \"agent_<UUID>\")."
                    },
                    "public_key": {
                        "type": "string",
                        "description": "Ed25519 public key in hex-encoded form (64 hex characters)."
                    },
                    "profile_name": {
                        "type": "string",
                        "description": "Human-readable display name for the agent."
                    },
                    "profile_description": {
                        "type": "string",
                        "description": "Longer description of the agent's purpose or capabilities."
                    }
                },
                "required": ["agent_id", "public_key"],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "send_message",
            description: "Send a message from one agent to another agent's mailbox.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from_agent_id": {
                        "type": "string",
                        "description": "The sending agent's ID."
                    },
                    "to_agent_id": {
                        "type": "string",
                        "description": "The receiving agent's ID."
                    },
                    "message_type": {
                        "type": "string",
                        "description": "Message type string."
                    },
                    "payload": {
                        "type": "object",
                        "description": "Arbitrary JSON payload carried by the message."
                    },
                    "expires_at": {
                        "type": "string",
                        "description": "Optional RFC 3339 timestamp after which the message should be ignored."
                    }
                },
                "required": ["from_agent_id", "to_agent_id", "message_type", "payload"],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "create_task",
            description: "Create a new task contract with capability, input, and verification parameters.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "requester": {
                        "type": "string",
                        "description": "Agent ID of the task requester."
                    },
                    "capability": {
                        "type": "string",
                        "description": "Capability required to execute the task (e.g. \"fact_verification\")."
                    },
                    "description": {
                        "type": "string",
                        "description": "Human-readable description of the work."
                    },
                    "input": {
                        "type": "object",
                        "description": "JSON-encoded input data for the task executor."
                    },
                    "deadline": {
                        "type": "string",
                        "description": "Optional RFC 3339 deadline for task completion."
                    },
                    "verification_method": {
                        "type": "string",
                        "enum": ["peer", "requester", "deterministic"],
                        "description": "Verification method (default: \"peer\")."
                    },
                    "required_validators": {
                        "type": "integer",
                        "description": "Number of validator approvals required (default: 2)."
                    }
                },
                "required": ["requester", "capability", "description", "input"],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "submit_result",
            description: "Submit a task result for validation.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to submit a result for."
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Agent ID of the executor submitting the result."
                    },
                    "result": {
                        "type": "object",
                        "description": "JSON-encoded result data."
                    },
                    "output_hash": {
                        "type": "string",
                        "description": "SHA-256 hash of the output for tamper-evident verification."
                    }
                },
                "required": ["task_id", "agent_id", "result", "output_hash"],
                "additionalProperties": false
            }),
        },
        McpTool {
            name: "validate",
            description: "Submit a validation decision (approve/reject) for a task.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "The task ID to validate."
                    },
                    "validator_id": {
                        "type": "string",
                        "description": "The validator agent's ID."
                    },
                    "decision": {
                        "type": "string",
                        "enum": ["approve", "reject"],
                        "description": "The validation decision."
                    },
                    "reasoning": {
                        "type": "string",
                        "description": "Optional reasoning for the decision."
                    }
                },
                "required": ["task_id", "validator_id", "decision"],
                "additionalProperties": false
            }),
        },
    ]
}
