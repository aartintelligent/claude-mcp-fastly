//! `list_service_domains` tool: list the domains of a specific Fastly
//! service version (`service_id` + `version`).
//!
//! The version is provided by the caller — typically obtained from
//! `get_service` first when the agent only knows a `service_id`. Same
//! contract as [`super::list_service_backends`] and
//! [`super::list_service_healthchecks`].

use fastly_api::apis::Error;
use fastly_api::apis::domain_api::{ListDomainsParams, list_domains};
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
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`DomainResponse`].
///
/// `DomainResponse` is already small (FQDN + optional comment + context).
/// We keep `name` and the creation/update timestamps; we drop `comment`,
/// `deleted_at`, and the redundant `service_id`/`version` that the caller
/// already has.
#[derive(Serialize)]
struct DomainSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> DomainSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `d`.
    fn from_response(d: &'a DomainResponse) -> Self {
        Self {
            name: d.name.as_deref(),
            created_at: d.created_at.as_deref(),
            updated_at: d.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim domain summaries for `service_id`@`version`.
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
    args: ListServiceDomainsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let domains = match list_domains(
        &mut cfg,
        ListDomainsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(d) => d,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No domains found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_domains failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<DomainSummary> = domains.iter().map(DomainSummary::from_response).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
