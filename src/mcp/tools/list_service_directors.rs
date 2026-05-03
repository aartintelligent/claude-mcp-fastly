//! `list_service_directors` tool: list the directors of a specific Fastly
//! service version (`service_id` + `version`).
//!
//! A director is a load-balancing group that bundles several backends
//! together with a balancing strategy (random, hash, client-sticky), an
//! up-quorum threshold, optional origin shielding, and a retry budget.
//! Fastly catalogs directors under "load balancing" rather than
//! "vcl-services" — they are accessible on every service kind, though
//! they only carry runtime meaning for VCL services. Same contract as the
//! other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::director_api::{ListDirectorsParams, list_directors};
use fastly_api::models::DirectorResponse;
use fastly_api::models::director_response;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_directors` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceDirectorsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`DirectorResponse`].
///
/// Mirrors the operationally meaningful fields documented at
/// <https://www.fastly.com/documentation/reference/api/load-balancing/directors/director/>,
/// plus the creation/update timestamps. Drops `comment`, `capacity`
/// (documented upstream as "Unused"), `deleted_at`, and the caller-known
/// context fields (`service_id`, `version`).
///
/// `backends` is projected from the embedded `Vec<Backend>` to a simple
/// `Vec<&str>` of backend names — the agent can chain
/// `list_service_backends` if it needs the full backend definitions.
///
/// Note: Fastly returns `type` as a numeric string — `"1"` (random),
/// `"3"` (hash), or `"4"` (client) — reflecting the underlying load-
/// balancing strategy.
#[derive(Serialize)]
struct DirectorSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    director_type: Option<&'a director_response::Type>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quorum: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retries: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shield: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backends: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> DirectorSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `d`.
    fn from_response(d: &'a DirectorResponse) -> Self {
        Self {
            name: d.name.as_deref(),
            director_type: d._type.as_ref(),
            quorum: d.quorum,
            retries: d.retries,
            shield: d.shield.as_deref(),
            backends: d
                .backends
                .as_deref()
                .map(|bs| bs.iter().filter_map(|b| b.name.as_deref()).collect()),
            created_at: d.created_at.as_deref(),
            updated_at: d.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim director summaries for
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
    args: ListServiceDirectorsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let directors = match list_directors(
        &mut cfg,
        ListDirectorsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(d) => d,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No directors found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_directors failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<DirectorSummary> = directors
        .iter()
        .map(DirectorSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
