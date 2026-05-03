//! `list_service_backends` tool: list the backends attached to a Fastly
//! service's currently-active version.
//!
//! Fastly's `list_backends` endpoint is version-scoped (`service_id` +
//! `version`), so we first resolve the service's active version with
//! `get_service`, then call `list_backends` for that version. Two API calls
//! per invocation; the active-version lookup is unavoidable as long as the
//! tool only takes `service_id`.

use fastly_api::apis::Error;
use fastly_api::apis::backend_api::{ListBackendsParams, list_backends};
use fastly_api::apis::service_api::{GetServiceParams, get_service};
use fastly_api::models::BackendResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_backends` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceBackendsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
}

/// Slimmed-down view of a Fastly [`BackendResponse`].
///
/// Drops the bulk of the upstream payload (TLS knobs, TCP keepalive timers,
/// timeouts, ssl_* fields, …) and keeps only what an agent typically needs
/// to reason about routing and load balancing.
#[derive(Serialize)]
struct BackendSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_ssl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shield: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_loadbalance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    healthcheck: Option<&'a str>,
}

impl<'a> BackendSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `b`.
    fn from_response(b: &'a BackendResponse) -> Self {
        Self {
            name: b.name.as_deref(),
            address: b.address.as_deref(),
            port: b.port,
            hostname: b.hostname.as_deref(),
            use_ssl: b.use_ssl,
            shield: b.shield.as_deref(),
            weight: b.weight,
            auto_loadbalance: b.auto_loadbalance,
            healthcheck: b.healthcheck.as_deref(),
        }
    }
}

/// Wrapper carrying the resolved version alongside the backend list.
///
/// Knowing the version answers "which configuration are these backends
/// from?" without the agent having to chain another lookup.
#[derive(Serialize)]
struct ServiceBackendsResponse<'a> {
    service_id: &'a str,
    version: i32,
    backends: Vec<BackendSummary<'a>>,
}

/// Resolves the active version of `service_id`, then lists its backends.
///
/// Returns a JSON object `{ service_id, version, backends[] }`. Plain-text
/// success results are returned in two specific cases that are not errors:
///
/// - the service id is unknown (`404` from `get_service`);
/// - the service exists but has no active version (e.g. brand-new, never
///   deployed).
///
/// # Errors
///
/// Returns an MCP internal error if either Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceBackendsArgs,
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

    // 2. List backends for that version.
    let backends = list_backends(
        &mut cfg,
        ListBackendsParams {
            service_id: args.service_id.clone(),
            version_id: version,
        },
    )
    .await
    .map_err(|e| {
        McpError::internal_error(format!("Fastly list_backends failed: {e}"), None)
    })?;

    // 3. Project to slim summaries (borrowing from `backends`).
    let summaries: Vec<BackendSummary> = backends.iter().map(BackendSummary::from_response).collect();

    let response = ServiceBackendsResponse {
        service_id: &args.service_id,
        version,
        backends: summaries,
    };

    Ok(CallToolResult::success(vec![Content::json(&response)?]))
}
