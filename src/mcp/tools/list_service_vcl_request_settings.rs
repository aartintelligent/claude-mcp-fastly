//! `list_service_vcl_request_settings` tool: list the request-settings
//! rules of a specific Fastly VCL service version (`service_id` +
//! `version`).
//!
//! A request settings entry tweaks how Fastly processes an incoming
//! request — host header, hash keys, X-Forwarded-For handling, force-SSL,
//! force-miss, etc. — optionally gated by a request condition. This is a
//! VCL-service feature — hence the `_vcl_` segment in the tool name —
//! Compute services configure these in user code instead. Same contract
//! as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::request_settings_api::{ListRequestSettingsParams, list_request_settings};
use fastly_api::models::RequestSettingsResponse;
use fastly_api::models::request_settings_response;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_request_settings` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclRequestSettingsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`RequestSettingsResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/request-settings/>,
/// plus the creation/update timestamps. Drops `deleted_at` and the
/// caller-known context fields (`service_id`, `version`).
///
/// Note: Fastly returns several boolean-like flags as numeric strings
/// (`"0"` / `"1"`) — `bypass_busy_wait`, `force_miss`, `force_ssl`,
/// `geo_headers`, `timer_support` — and we forward them as-is.
#[derive(Serialize)]
struct RequestSettingsSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a request_settings_response::Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_host: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash_keys: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    xff: Option<&'a request_settings_response::Xff>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bypass_busy_wait: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force_miss: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force_ssl: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    geo_headers: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_stale_age: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timer_support: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> RequestSettingsSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `r`.
    fn from_response(r: &'a RequestSettingsResponse) -> Self {
        Self {
            name: r.name.as_deref(),
            action: r.action.as_ref(),
            request_condition: r.request_condition.as_deref(),
            default_host: r.default_host.as_deref(),
            hash_keys: r.hash_keys.as_deref(),
            xff: r.xff.as_ref(),
            bypass_busy_wait: r.bypass_busy_wait.as_deref(),
            force_miss: r.force_miss.as_deref(),
            force_ssl: r.force_ssl.as_deref(),
            geo_headers: r.geo_headers.as_deref(),
            max_stale_age: r.max_stale_age.as_deref(),
            timer_support: r.timer_support.as_deref(),
            created_at: r.created_at.as_deref(),
            updated_at: r.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim request-settings summaries for
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
    args: ListServiceVclRequestSettingsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let request_settings = match list_request_settings(
        &mut cfg,
        ListRequestSettingsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No request settings found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_request_settings failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<RequestSettingsSummary> = request_settings
        .iter()
        .map(RequestSettingsSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
