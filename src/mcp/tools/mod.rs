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

pub mod find_domain;
pub mod find_resource_acl_entry;
pub mod get_resource_config_store_item_value;
pub mod get_resource_kv_store_item_value;
pub mod get_service;
pub mod get_service_package;
pub mod list_resource_acls;
pub mod list_resource_config_store_items;
pub mod list_resource_config_stores;
pub mod list_resource_kv_store_items;
pub mod list_resource_kv_stores;
pub mod list_resource_secret_store_items;
pub mod list_resource_secret_stores;
pub mod list_service_backends;
pub mod list_service_dictionaries;
pub mod list_service_dictionary_items;
pub mod list_service_directors;
pub mod list_service_domains;
pub mod list_service_healthchecks;
pub mod list_service_resources;
pub mod list_service_vcl_apex_redirects;
pub mod list_service_vcl_cache_settings;
pub mod list_service_vcl_conditions;
pub mod list_service_vcl_gzip;
pub mod list_service_vcl_headers;
pub mod list_service_vcl_rate_limiters;
pub mod list_service_vcl_request_settings;
pub mod list_service_vcl_response_objects;
pub mod list_service_vcl_snippets;
pub mod list_service_versions;
