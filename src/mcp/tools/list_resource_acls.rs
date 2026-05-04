//! `list_resource_acls` tool: list the Fastly account's Compute ACLs.
//!
//! Compute ACLs (a.k.a. "Resource ACLs", under the Fastly docs path
//! `/api/acls/compute-acl/`) are an **account-scoped** resource designed
//! to hold large lists of CIDR rules — orders of magnitude bigger than
//! the legacy version-scoped VCL ACLs. They are referenced from Compute
//! services through dedicated SDK bindings, not from VCL.
//!
//! This tool returns the catalog only. Individual entries are *not*
//! enumerated — that listing is intentionally avoided because a single
//! Compute ACL can hold millions of prefixes. To check whether a specific
//! IP is covered by an ACL, use [`super::find_resource_acl_entry`], which
//! relies on Fastly's dedicated lookup endpoint (no scan).

use fastly_api::apis::acls_in_compute_api::compute_acl_list_acls;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// This tool takes no arguments — Fastly returns the full Compute ACL
/// catalog in a single call. The struct exists only so the rmcp parameter
/// extractor and `schemars` schema generator agree on the shape.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListResourceAclsArgs {}

/// One Compute ACL entry in the catalog. Both fields are mirrored from
/// the upstream `ComputeAclCreateAclsResponse` — Fastly intentionally
/// keeps Compute ACL metadata minimal (no timestamps).
#[derive(Serialize)]
struct AclSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
}

/// Wrapper carrying the full catalog plus the upstream-reported `total`,
/// which is convenient for the agent to confirm pagination is unnecessary.
#[derive(Serialize)]
struct ListAclsResponse<'a> {
    acls: Vec<AclSummary<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<i32>,
}

/// Returns the catalog of Compute ACLs in the account.
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails (network, auth,
/// deserialization, 5xx).
pub async fn run(
    state: &AppState,
    _args: ListResourceAclsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = compute_acl_list_acls(&mut cfg).await.map_err(|e| {
        McpError::internal_error(format!("Fastly compute_acl_list_acls failed: {e}"), None)
    })?;

    let acls: Vec<AclSummary> = response
        .data
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|a| AclSummary {
            id: a.id.as_deref(),
            name: a.name.as_deref(),
        })
        .collect();

    let total = response.meta.as_deref().and_then(|m| m.total);

    Ok(CallToolResult::success(vec![Content::json(
        &ListAclsResponse { acls, total },
    )?]))
}
