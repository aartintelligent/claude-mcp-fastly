//! `list_service_vcl_cache_settings` tool: list the cache-settings rules of
//! a specific Fastly VCL service version (`service_id` + `version`).
//!
//! A cache settings entry tells `vcl_fetch` how to treat a response: keep
//! it (`cache`), bypass the cache (`pass`), or restart (`restart`). It can
//! optionally override the TTL and stale-if-error window for matching
//! responses, gated by a `cache_condition`. This is a VCL-service feature
//! — hence the `_vcl_` segment in the tool name — it does not exist on
//! Compute services. Same contract as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::cache_settings_api::{ListCacheSettingsParams, list_cache_settings};
use fastly_api::models::CacheSettingResponse;
use fastly_api::models::cache_setting_response::Action as CacheSettingAction;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_cache_settings` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclCacheSettingsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`CacheSettingResponse`].
///
/// Mirrors every configurable field documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/cache-settings/>,
/// plus the creation/update timestamps. Drops `deleted_at` and the
/// caller-known context fields (`service_id`, `version`).
///
/// Note: Fastly returns `ttl` and `stale_ttl` as strings (numeric values
/// wrapped in quotes), and we forward them as-is.
#[derive(Serialize)]
struct CacheSettingSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a CacheSettingAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stale_ttl: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> CacheSettingSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `c`.
    fn from_response(c: &'a CacheSettingResponse) -> Self {
        Self {
            name: c.name.as_deref(),
            action: c.action.as_ref(),
            cache_condition: c.cache_condition.as_deref(),
            ttl: c.ttl.as_deref(),
            stale_ttl: c.stale_ttl.as_deref(),
            created_at: c.created_at.as_deref(),
            updated_at: c.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim cache-settings summaries for
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
    args: ListServiceVclCacheSettingsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let cache_settings = match list_cache_settings(
        &mut cfg,
        ListCacheSettingsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(c) => c,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No cache settings found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_cache_settings failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<CacheSettingSummary> = cache_settings
        .iter()
        .map(CacheSettingSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
