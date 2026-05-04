//! `find_resource_acl_entry` tool: look up the entry of a Compute ACL
//! that covers a given IP address.
//!
//! Backed by `GET /resources/acls/{acl_id}/lookup?ip={ip}`. This is
//! Fastly's dedicated lookup endpoint — it returns the matching CIDR
//! prefix and its action (`ALLOW` / `BLOCK`) in **a single call**, with
//! no need to enumerate the (potentially millions of) entries an ACL
//! holds. That's the whole reason this tool exists rather than a
//! `list_resource_acl_entries` companion: paginating a multi-million row
//! ACL is exactly the cost we want to avoid.
//!
//! On a 404 (either the ACL does not exist *or* no entry covers the
//! provided IP — Fastly does not distinguish), the tool downgrades to a
//! plain-text message instead of an MCP error so the agent can act on a
//! clean signal.

use fastly_api::apis::Error;
use fastly_api::apis::acls_in_compute_api::{ComputeAclLookupAclsParams, compute_acl_lookup_acls};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `find_resource_acl_entry` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindResourceAclEntryArgs {
    /// Compute ACL identifier — typically the `id` field of an entry
    /// returned by `list_resource_acls`.
    pub acl_id: String,
    /// IPv4 or IPv6 address to look up against the ACL. Fastly returns
    /// the *most specific* CIDR prefix that covers this IP, along with
    /// its action.
    pub ip: String,
}

/// Result wrapper carrying the input context plus the matched
/// prefix/action so the agent can summarize without re-quoting its own
/// inputs.
#[derive(Serialize)]
struct AclLookupResult<'a> {
    acl_id: &'a str,
    ip: &'a str,
    /// CIDR prefix of the matching entry (e.g. `203.0.113.0/24`).
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<&'a str>,
    /// Action of the matching entry — typically `"ALLOW"` or `"BLOCK"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a str>,
}

/// Returns the entry of `acl_id` that covers `ip`, or a plain-text
/// "no match" message on 404 (which covers both unknown ACL id and
/// unmatched IP — Fastly does not differentiate at this endpoint).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: FindResourceAclEntryArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let lookup = match compute_acl_lookup_acls(
        &mut cfg,
        ComputeAclLookupAclsParams {
            acl_id: args.acl_id.clone(),
            acl_ip: args.ip.clone(),
        },
    )
    .await
    {
        Ok(l) => l,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No entry in ACL `{}` matches IP `{}` (or the ACL does not exist).",
                args.acl_id, args.ip
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly compute_acl_lookup_acls failed: {e}"),
                None,
            ));
        }
    };

    Ok(CallToolResult::success(vec![Content::json(
        &AclLookupResult {
            acl_id: &args.acl_id,
            ip: &args.ip,
            prefix: lookup.prefix.as_deref(),
            action: lookup.action.as_deref(),
        },
    )?]))
}
