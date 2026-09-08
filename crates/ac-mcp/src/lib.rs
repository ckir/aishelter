//! MCP (Model Context Protocol) adapter for Agent Commons v1.0.
//!
//! Translates incoming MCP JSON-RPC 2.0 requests into ACP/1 internal
//! service calls and maps responses back.  MCP adapters MUST NOT become
//! the canonical storage model — ACP/1 + HTTP/OpenAPI remain canonical.
//!
//! # Usage
//!
//! ```ignore
//! use ac_db::pool::SharedPool;
//! use ac_mcp::mcp_router;
//!
//! let app = axum::Router::new()
//!     .nest("/mcp", mcp_router(pool));
//! ```
//!
//! # Supported MCP methods
//!
//! | MCP method | ACP/1 operation |
//! |---|---|
//! | `tools/list` | Return available MCP tools |
//! | `tools/call` → `search_agents` | ac-discovery search |
//! | `tools/call` → `register_agent` | ac-registry register |
//! | `tools/call` → `send_message` | ac-mailbox send |
//! | `tools/call` → `create_task` | ac-tasks create |
//! | `tools/call` → `submit_result` | ac-tasks submit |
//! | `tools/call` → `validate` | ac-validation validate |

/// MCP JSON-RPC 2.0 router and request handler.
pub mod handler;

/// MCP tool declarations and input schemas.
pub mod tools;

pub use handler::{JsonRpcResponse, mcp_router};
