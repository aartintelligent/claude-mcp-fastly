//! `list_service_vcl_apex_redirects` tool: list the apex redirects of a
//! specific Fastly VCL service version (`service_id` + `version`).
//!
//! An apex redirect tells Fastly to send a `30x` to a configured WWW
//! subdomain when the client requests one of the listed apex domains. This
//! is a VCL-service feature — hence the `_vcl_` segment in the tool name —
//! it does not exist on Compute services. Same contract as the other
//! `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::apex_redirect_api::{ListApexRedirectsParams, list_apex_redirects};
use fastly_api::models::ApexRedirect;
use fastly_api::models::apex_redirect::StatusCode as ApexRedirectStatusCode;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_apex_redirects` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclApexRedirectsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`ApexRedirect`].
///
/// Keeps the redirect intent (`status_code`, `domains`) and the
/// creation/update timestamps. Drops `feature_revision` (internal Fastly
/// bookkeeping), `deleted_at`, and the caller-known context fields
/// (`service_id`, `version`). Fastly's SDK does not expose a stable id for
/// apex redirects — they are identified within a version by their
/// `domains` set.
#[derive(Serialize)]
struct ApexRedirectSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<&'a ApexRedirectStatusCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domains: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> ApexRedirectSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `r`.
    fn from_response(r: &'a ApexRedirect) -> Self {
        Self {
            status_code: r.status_code.as_ref(),
            domains: r.domains.as_deref(),
            created_at: r.created_at.as_deref(),
            updated_at: r.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim apex-redirect summaries for
/// `service_id`@`version`.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message,
/// covering both unknown service id and unknown version.
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceVclApexRedirectsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let redirects = match list_apex_redirects(
        &mut cfg,
        ListApexRedirectsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No apex redirects found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_apex_redirects failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<ApexRedirectSummary> = redirects
        .iter()
        .map(ApexRedirectSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
