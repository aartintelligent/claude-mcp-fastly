//! `list_service_healthchecks` tool: list the healthchecks attached to a
//! Fastly service's currently-active version.
//!
//! Fastly's `list_healthchecks` endpoint is version-scoped (`service_id` +
//! `version`), so we first resolve the service's active version with
//! `get_service`, then call `list_healthchecks` for that version. Two API
//! calls per invocation — same pattern as
//! [`super::list_service_backends`].

use fastly_api::apis::Error;
use fastly_api::apis::healthcheck_api::{ListHealthchecksParams, list_healthchecks};
use fastly_api::apis::service_api::{GetServiceParams, get_service};
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
}

/// Slimmed-down view of a Fastly [`HealthcheckResponse`].
///
/// Drops `headers`, `comment`, and the timestamps/context fields. Keeps the
/// probe definition (host/path/method/...) and the decision math
/// (interval/timeout/window/threshold/initial) — what an SRE actually
/// reads when troubleshooting a sick backend.
#[derive(Serialize)]
struct HealthcheckSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_response: Option<i32>,
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
            expected_response: h.expected_response,
            check_interval: h.check_interval,
            timeout: h.timeout,
            window: h.window,
            threshold: h.threshold,
            initial: h.initial,
        }
    }
}

/// Wrapper carrying the resolved version alongside the healthcheck list.
#[derive(Serialize)]
struct ServiceHealthchecksResponse<'a> {
    service_id: &'a str,
    version: i32,
    healthchecks: Vec<HealthcheckSummary<'a>>,
}

/// Resolves the active version of `service_id`, then lists its healthchecks.
///
/// Returns a JSON object `{ service_id, version, healthchecks[] }`. Plain-text
/// success results are returned in two specific cases that are not errors:
///
/// - the service id is unknown (`404` from `get_service`);
/// - the service exists but has no active version.
///
/// # Errors
///
/// Returns an MCP internal error if either Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceHealthchecksArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    // 1. Resolve the active version via get_service.
    let svc = match get_service(
        &mut cfg,
        GetServiceParams {
            service_id: args.service_id.clone(),
        },
    )
    .await
    {
        Ok(s) => s,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No service found with id `{}`.",
                args.service_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly get_service failed: {e}"),
                None,
            ));
        }
    };

    let Some(version) = svc
        .versions
        .as_deref()
        .and_then(|v| v.iter().find(|ver| ver.active == Some(true)))
        .and_then(|ver| ver.number)
    else {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "Service `{}` has no active version.",
            args.service_id
        ))]));
    };

    // 2. List healthchecks for that version.
    let healthchecks = list_healthchecks(
        &mut cfg,
        ListHealthchecksParams {
            service_id: args.service_id.clone(),
            version_id: version,
        },
    )
    .await
    .map_err(|e| {
        McpError::internal_error(format!("Fastly list_healthchecks failed: {e}"), None)
    })?;

    // 3. Project to slim summaries (borrowing from `healthchecks`).
    let summaries: Vec<HealthcheckSummary> = healthchecks
        .iter()
        .map(HealthcheckSummary::from_response)
        .collect();

    let response = ServiceHealthchecksResponse {
        service_id: &args.service_id,
        version,
        healthchecks: summaries,
    };

    Ok(CallToolResult::success(vec![Content::json(&response)?]))
}
