//! `find_domain` tool: account-scoped lookup of a domain by FQDN.
//!
//! Backed by Fastly's Domain Management v1 list endpoint
//! (<https://www.fastly.com/documentation/reference/api/domain-management/domains/>),
//! which is *not* version-scoped — it returns first-class domain entries
//! across the whole Fastly account, with their associated `service_id`
//! (or `null` if the domain isn't yet bound to a service).
//!
//! Useful as an entry point when the agent only knows a hostname and
//! needs to discover which service serves it (or whether it's owned by
//! the account at all).

use fastly_api::apis::dm_domains_api::{ListDmDomainsParams, list_dm_domains};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `find_domain` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindDomainArgs {
    /// Fully-qualified domain name to look up (e.g. `www.example.com`).
    pub fqdn: String,
    /// Optional match type for `fqdn`. Accepts `exact`, `contains`,
    /// `begins_with`, or `ends_with`. When omitted, Fastly applies its
    /// default permissive match (which can return more than the exact
    /// FQDN — e.g. its sub-domains).
    #[serde(default)]
    pub fqdn_match: Option<String>,
}

/// Slimmed-down view of a Fastly Domain Management v1 domain entry.
///
/// Mirrors the documented response shape minus `description` (freeform
/// note, dropped for consistency with other slim views).
#[derive(Serialize)]
struct DomainSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fqdn: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Looks up domains in the Fastly account by FQDN.
///
/// Forwards the FQDN to Fastly's `?fqdn=` filter so the matching happens
/// server-side; we never paginate locally. The response is a JSON array
/// of slim domain entries — typically zero or one match for an exact
/// FQDN. When no domain matches, a plain-text "not found" message is
/// returned instead of an empty array, so the agent gets an immediately
/// readable signal.
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails (network, auth,
/// deserialization).
pub async fn run(state: &AppState, args: FindDomainArgs) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = list_dm_domains(
        &mut cfg,
        ListDmDomainsParams {
            fqdn: Some(args.fqdn.clone()),
            fqdn_match: args.fqdn_match.clone(),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| McpError::internal_error(format!("Fastly list_dm_domains failed: {e}"), None))?;

    let domains = response.data.unwrap_or_default();

    if domains.is_empty() {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "No domain found matching `{}`.",
            args.fqdn
        ))]));
    }

    let summaries: Vec<DomainSummary> = domains
        .iter()
        .map(|d| DomainSummary {
            id: d.id.as_deref(),
            fqdn: d.fqdn.as_deref(),
            service_id: d.service_id.as_deref(),
            activated: d.activated,
            verified: d.verified,
            created_at: d.created_at.as_deref(),
            updated_at: d.updated_at.as_deref(),
        })
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
