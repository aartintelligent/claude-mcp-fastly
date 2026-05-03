//! `list_service_vcl_gzip` tool: list the gzip configurations of a specific
//! Fastly VCL service version (`service_id` + `version`).
//!
//! A gzip configuration tells Fastly which MIME types and file extensions
//! to compress on the fly when serving from cache, optionally gated by a
//! cache condition. This is a VCL-service feature — hence the `_vcl_`
//! segment in the tool name — Compute services handle compression in user
//! code instead. Same contract as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::gzip_api::{ListGzipConfigsParams, list_gzip_configs};
use fastly_api::models::GzipResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_gzip` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclGzipArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`GzipResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/gzip/>,
/// plus the creation/update timestamps. Drops `deleted_at` and the
/// caller-known context fields (`service_id`, `version`).
///
/// Note: `content_types` and `extensions` are stored by Fastly as
/// space-separated strings, not arrays — we forward them as-is.
#[derive(Serialize)]
struct GzipSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_types: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extensions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> GzipSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `g`.
    fn from_response(g: &'a GzipResponse) -> Self {
        Self {
            name: g.name.as_deref(),
            cache_condition: g.cache_condition.as_deref(),
            content_types: g.content_types.as_deref(),
            extensions: g.extensions.as_deref(),
            created_at: g.created_at.as_deref(),
            updated_at: g.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim gzip summaries for `service_id`@`version`.
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
    args: ListServiceVclGzipArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let gzip_configs = match list_gzip_configs(
        &mut cfg,
        ListGzipConfigsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(g) => g,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No gzip configurations found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_gzip_configs failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<GzipSummary> = gzip_configs.iter().map(GzipSummary::from_response).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
