//! `list_service_healthchecks` tool: list the healthchecks of a specific
//! Fastly service version (`service_id` + `version`).
//!
//! The version is provided by the caller — typically obtained from
//! `get_service` first when the agent only knows a `service_id`. Same
//! contract as [`super::list_service_backends`].

use fastly_api::apis::Error;
use fastly_api::apis::healthcheck_api::{ListHealthchecksParams, list_healthchecks};
use fastly_api::models::HealthcheckResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_healthchecks` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceHealthchecksArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`HealthcheckResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/services/healthcheck/>,
/// grouped by concern: identity, probe shape (the HTTP request the probe
/// emits), decision math (how the probe outcome translates into a backend
/// health verdict), and the creation/update timestamps. Drops `comment`,
/// `deleted_at`, and the caller-known context fields (`service_id`,
/// `version`).
#[derive(Serialize)]
struct HealthcheckSummary<'a> {
    // identity
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,

    // probe shape
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_response: Option<i32>,

    // decision math
    #[serde(skip_serializing_if = "Option::is_none")]
    check_interval: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    threshold: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial: Option<i32>,

    // metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> HealthcheckSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `h`.
    fn from_response(h: &'a HealthcheckResponse) -> Self {
        Self {
            name: h.name.as_deref(),
            host: h.host.as_deref(),
            path: h.path.as_deref(),
            method: h.method.as_deref(),
            http_version: h.http_version.as_deref(),
            headers: h.headers.as_deref(),
            expected_response: h.expected_response,
            check_interval: h.check_interval,
            timeout: h.timeout,
            window: h.window,
            threshold: h.threshold,
            initial: h.initial,
            created_at: h.created_at.as_deref(),
            updated_at: h.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim healthcheck summaries for
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
    args: ListServiceHealthchecksArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let healthchecks = match list_healthchecks(
        &mut cfg,
        ListHealthchecksParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(h) => h,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No healthchecks found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_healthchecks failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<HealthcheckSummary> = healthchecks
        .iter()
        .map(HealthcheckSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
