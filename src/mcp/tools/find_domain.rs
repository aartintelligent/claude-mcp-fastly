//! `find_domain` tool: look up a domain in the Fastly account by FQDN.
//!
//! Backed by Fastly's Domain Management v1 API (`/domains/v1`), which is
//! account-scoped and exposes a server-side `fqdn` filter — so we forward the
//! match to Fastly rather than paginating client-side.

use fastly_api::apis::dm_domains_api::{ListDmDomainsParams, list_dm_domains};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::AppState;

/// Arguments accepted by the `find_domain` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindDomainArgs {
    /// Fully-qualified domain name to look up (e.g. `www.example.com`).
    pub fqdn: String,
}

/// Looks up domains by FQDN via Fastly's Domain Management v1 list endpoint.
///
/// Returns the matched domain entries (id, fqdn, associated service, activated
/// and verified flags) as JSON content. When no domain matches, a plain-text
/// "no match" message is returned instead — easier for the LLM to summarize
/// than an empty array.
///
/// # Errors
///
/// Returns an MCP internal error if the upstream Fastly call fails (network,
/// auth, deserialization).
pub async fn run(state: &AppState, args: FindDomainArgs) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let params = ListDmDomainsParams {
        fqdn: Some(args.fqdn.clone()),
        ..Default::default()
    };

    let response = list_dm_domains(&mut cfg, params)
        .await
        .map_err(|e| McpError::internal_error(format!("Fastly list_dm_domains failed: {e}"), None))?;

    let domains = response.data.unwrap_or_default();

    if domains.is_empty() {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "No domain found matching `{}`.",
            args.fqdn
        ))]));
    }

    Ok(CallToolResult::success(vec![Content::json(&domains)?]))
}
