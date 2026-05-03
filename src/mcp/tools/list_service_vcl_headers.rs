//! `list_service_vcl_headers` tool: list the header rules of a specific
//! Fastly VCL service version (`service_id` + `version`).
//!
//! A header rule mutates a request, cache, or response header — set,
//! append, delete, or regex-rewrite — optionally gated by a condition.
//! This is a VCL-service feature — hence the `_vcl_` segment in the tool
//! name — Compute services manipulate headers in user code instead. Same
//! contract as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::header_api::{ListHeaderObjectsParams, list_header_objects};
use fastly_api::models::HeaderResponse;
use fastly_api::models::header_response::{Action as HeaderAction, Type as HeaderType};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_headers` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclHeadersArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`HeaderResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/header/>,
/// plus the creation/update timestamps. Drops `deleted_at` and the
/// caller-known context fields (`service_id`, `version`).
///
/// Notes on Fastly types:
/// - `priority` and `ignore_if_set` are returned as numeric strings
///   (`"0"` / `"1"` etc.); we forward as-is.
/// - `regex` and `substitution` are only meaningful when `action` is
///   `regex` or `regex_repeat`; they are omitted otherwise via
///   `skip_serializing_if`.
#[derive(Serialize)]
struct HeaderSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    header_type: Option<&'a HeaderType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a HeaderAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dst: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    src: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    regex: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    substitution: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_if_set: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> HeaderSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `h`.
    fn from_response(h: &'a HeaderResponse) -> Self {
        Self {
            name: h.name.as_deref(),
            header_type: h._type.as_ref(),
            action: h.action.as_ref(),
            dst: h.dst.as_deref(),
            src: h.src.as_deref(),
            regex: h.regex.as_deref(),
            substitution: h.substitution.as_deref(),
            ignore_if_set: h.ignore_if_set.as_deref(),
            priority: h.priority.as_deref(),
            request_condition: h.request_condition.as_deref(),
            // `response_condition` is `Option<Box<String>>` upstream — deref
            // through the Box to a borrowed `&str`.
            response_condition: h.response_condition.as_deref().map(String::as_str),
            cache_condition: h.cache_condition.as_deref(),
            created_at: h.created_at.as_deref(),
            updated_at: h.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim header summaries for `service_id`@`version`.
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
    args: ListServiceVclHeadersArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let headers = match list_header_objects(
        &mut cfg,
        ListHeaderObjectsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(h) => h,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No headers found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_header_objects failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<HeaderSummary> = headers.iter().map(HeaderSummary::from_response).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
