//! MCP tool implementations.
//!
//! Each submodule corresponds to a single tool exposed to the client.
//! By convention every tool module declares:
//!
//! - a public `Args` struct deriving [`serde::Deserialize`] and
//!   [`schemars::JsonSchema`], used both by the rmcp parameter extractor
//!   and to generate the JSON schema advertised on `tools/list`;
//! - a public `run` async function taking `(&AppState, Args)` and returning
//!   `Result<CallToolResult, McpError>`.
//!
//! The `#[tool_router]` block in [`crate::mcp::handler::Handler`] keeps the
//! routing surface thin: it only declares the tool name and description,
//! then delegates to `run`.

pub mod get_service;
pub mod list_service_backends;
pub mod list_service_directors;
pub mod list_service_domains;
pub mod list_service_healthchecks;
pub mod list_service_versions;
pub mod list_service_vcl_apex_redirects;
pub mod list_service_vcl_cache_settings;
pub mod list_service_vcl_conditions;
pub mod list_service_vcl_gzip;
pub mod list_service_vcl_headers;
pub mod list_service_vcl_rate_limiters;
pub mod list_service_vcl_request_settings;
pub mod list_service_vcl_response_objects;
pub mod list_service_vcl_snippets;
