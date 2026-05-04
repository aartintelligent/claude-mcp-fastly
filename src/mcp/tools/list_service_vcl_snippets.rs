//! `list_service_vcl_snippets` tool: list the VCL snippets of a specific
//! Fastly VCL service version (`service_id` + `version`).
//!
//! A snippet is a fragment of VCL code injected into a specific phase of
//! Fastly's request lifecycle (`recv`, `hit`, `fetch`, `deliver`, …).
//! Snippets can be regular (versioned with the service config) or dynamic
//! (mutable without a config deploy). This is a VCL-service feature —
//! hence the `_vcl_` segment in the tool name — Compute services replace
//! VCL with user code entirely. Same contract as the other
//! `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::snippet_api::{ListSnippetsParams, list_snippets};
use fastly_api::models::SnippetResponse;
use fastly_api::models::snippet_response;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_snippets` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclSnippetsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`SnippetResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/snippet/>,
/// plus the snippet `id` and the creation/update timestamps. Drops
/// `deleted_at` and the caller-known context fields (`service_id`,
/// `version`).
///
/// Note: `content` carries the literal VCL source and can be sizeable for
/// non-trivial snippets — list output grows accordingly. `dynamic` is the
/// numeric string `"0"` (regular, versioned) or `"1"` (dynamic, mutable
/// out-of-band). `priority` is also a numeric string.
#[derive(Serialize)]
struct SnippetSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    snippet_type: Option<&'a snippet_response::Type>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dynamic: Option<&'a snippet_response::Dynamic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> SnippetSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `s`.
    fn from_response(s: &'a SnippetResponse) -> Self {
        Self {
            // `id` is `Option<Box<String>>` upstream — deref through the Box
            // to a borrowed `&str`.
            id: s.id.as_deref().map(String::as_str),
            name: s.name.as_deref(),
            snippet_type: s._type.as_ref(),
            dynamic: s.dynamic.as_ref(),
            priority: s.priority.as_deref(),
            content: s.content.as_deref(),
            created_at: s.created_at.as_deref(),
            updated_at: s.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim snippet summaries for
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
    args: ListServiceVclSnippetsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let snippets = match list_snippets(
        &mut cfg,
        ListSnippetsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(s) => s,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No snippets found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_snippets failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<SnippetSummary> =
        snippets.iter().map(SnippetSummary::from_response).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
