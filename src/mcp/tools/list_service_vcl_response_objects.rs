//! `list_service_vcl_response_objects` tool: list the response objects of
//! a specific Fastly VCL service version (`service_id` + `version`).
//!
//! A response object is a canned HTTP response (status, headers, body)
//! that Fastly serves directly without going to origin — typical uses are
//! custom error pages, maintenance modes, or rate-limit replies. This is
//! a VCL-service feature — hence the `_vcl_` segment in the tool name —
//! Compute services synthesize responses in user code instead. Same
//! contract as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::response_object_api::{ListResponseObjectsParams, list_response_objects};
use fastly_api::models::ResponseObjectResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_response_objects` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclResponseObjectsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`ResponseObjectResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/response-object/>,
/// plus the creation/update timestamps. Drops `deleted_at` and the
/// caller-known context fields (`service_id`, `version`).
///
/// Note: Fastly returns `status` as a numeric string (e.g. `"503"`) — we
/// forward as-is. `content` may contain arbitrarily large bodies (custom
/// error pages, etc.); list calls usually have only a handful of objects
/// so this is acceptable, but expect the JSON to grow with the size of
/// the configured payloads.
#[derive(Serialize)]
struct ResponseObjectSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> ResponseObjectSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `r`.
    fn from_response(r: &'a ResponseObjectResponse) -> Self {
        Self {
            name: r.name.as_deref(),
            status: r.status.as_deref(),
            response: r.response.as_deref(),
            content_type: r.content_type.as_deref(),
            content: r.content.as_deref(),
            request_condition: r.request_condition.as_deref(),
            cache_condition: r.cache_condition.as_deref(),
            created_at: r.created_at.as_deref(),
            updated_at: r.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim response-object summaries for
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
    args: ListServiceVclResponseObjectsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response_objects = match list_response_objects(
        &mut cfg,
        ListResponseObjectsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No response objects found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_response_objects failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<ResponseObjectSummary> = response_objects
        .iter()
        .map(ResponseObjectSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
