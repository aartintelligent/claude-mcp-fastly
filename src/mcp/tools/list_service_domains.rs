//! `list_service_domains` tool: list the domains attached to a Fastly
//! service's currently-active version.
//!
//! Fastly's `list_domains` endpoint is version-scoped (`service_id` +
//! `version`), so we first resolve the service's active version with
//! `get_service`, then call `list_domains` for that version. Two API calls
//! per invocation — same pattern as [`super::list_service_backends`] and
//! [`super::list_service_healthchecks`].

use fastly_api::apis::Error;
use fastly_api::apis::domain_api::{ListDomainsParams, list_domains};
use fastly_api::apis::service_api::{GetServiceParams, get_service};
use fastly_api::models::DomainResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_domains` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceDomainsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
}

/// Slimmed-down view of a Fastly [`DomainResponse`].
///
/// `DomainResponse` is already small (FQDN + optional comment + context).
/// We keep `name` and `comment`, drop the timestamps and redundant
/// service_id/version which are already present at the wrapper level.
#[derive(Serialize)]
struct DomainSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<&'a str>,
}

impl<'a> DomainSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `d`.
    fn from_response(d: &'a DomainResponse) -> Self {
        Self {
            name: d.name.as_deref(),
            comment: d.comment.as_deref(),
        }
    }
}

/// Wrapper carrying the resolved version alongside the domain list.
#[derive(Serialize)]
struct ServiceDomainsResponse<'a> {
    service_id: &'a str,
    version: i32,
    domains: Vec<DomainSummary<'a>>,
}

/// Resolves the active version of `service_id`, then lists its domains.
///
/// Returns a JSON object `{ service_id, version, domains[] }`. Plain-text
/// success results are returned in two specific cases that are not errors:
///
/// - the service id is unknown (`404` from `get_service`);
/// - the service exists but has no active version.
///
/// # Errors
///
/// Returns an MCP internal error if either Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceDomainsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    // 1. Resolve the active version via get_service.
    let svc = match get_service(
        &mut cfg,
        GetServiceParams {
            service_id: args.service_id.clone(),
        },
    )
    .await
    {
        Ok(s) => s,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No service found with id `{}`.",
                args.service_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly get_service failed: {e}"),
                None,
            ));
        }
    };

    let Some(version) = svc
        .versions
        .as_deref()
        .and_then(|v| v.iter().find(|ver| ver.active == Some(true)))
        .and_then(|ver| ver.number)
    else {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "Service `{}` has no active version.",
            args.service_id
        ))]));
    };

    // 2. List domains for that version.
    let domains = list_domains(
        &mut cfg,
        ListDomainsParams {
            service_id: args.service_id.clone(),
            version_id: version,
        },
    )
    .await
    .map_err(|e| {
        McpError::internal_error(format!("Fastly list_domains failed: {e}"), None)
    })?;

    // 3. Project to slim summaries (borrowing from `domains`).
    let summaries: Vec<DomainSummary> = domains.iter().map(DomainSummary::from_response).collect();

    let response = ServiceDomainsResponse {
        service_id: &args.service_id,
        version,
        domains: summaries,
    };

    Ok(CallToolResult::success(vec![Content::json(&response)?]))
}
