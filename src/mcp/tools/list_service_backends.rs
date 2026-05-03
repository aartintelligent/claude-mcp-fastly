//! `list_service_backends` tool: list the backends of a specific
//! Fastly service version (`service_id` + `version`).
//!
//! The version is provided by the caller — typically obtained from
//! `get_service` first when the agent only knows a `service_id`. Keeping
//! the version explicit avoids a redundant `get_service` round-trip on
//! every invocation and lets the agent inspect historical versions.

use fastly_api::apis::Error;
use fastly_api::apis::backend_api::{ListBackendsParams, list_backends};
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
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`BackendResponse`].
///
/// Keeps the operationally meaningful subset of the upstream payload
/// grouped by concern: identity, target, TLS posture, routing/LB, health,
/// the main timeouts, and the creation/update timestamps. Drops the
/// lowest-value knobs (`comment`, TCP keepalive timers, `fetch_timeout`,
/// deprecated `ssl_hostname`, `ssl_ciphers`, `ssl_ca_cert`, `ssl_client_*`,
/// `ipv4`/`ipv6` redundant with `address`, and `client_cert` documented as
/// unused).
#[derive(Serialize)]
struct BackendSummary<'a> {
    // identity
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,

    // connection target
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    override_host: Option<&'a str>,

    // TLS posture
    #[serde(skip_serializing_if = "Option::is_none")]
    use_ssl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssl_check_cert: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_tls_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tls_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssl_cert_hostname: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssl_sni_hostname: Option<&'a str>,

    // routing & load balancing
    #[serde(skip_serializing_if = "Option::is_none")]
    request_condition: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_loadbalance: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shield: Option<&'a str>,

    // health
    #[serde(skip_serializing_if = "Option::is_none")]
    healthcheck: Option<&'a str>,

    // timeouts & connection pool
    #[serde(skip_serializing_if = "Option::is_none")]
    connect_timeout: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_byte_timeout: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    between_bytes_timeout: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_conn: Option<i32>,

    // metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> BackendSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `b`.
    fn from_response(b: &'a BackendResponse) -> Self {
        Self {
            name: b.name.as_deref(),
            address: b.address.as_deref(),
            port: b.port,
            hostname: b.hostname.as_deref(),
            override_host: b.override_host.as_deref(),
            use_ssl: b.use_ssl,
            ssl_check_cert: b.ssl_check_cert,
            min_tls_version: b.min_tls_version.as_deref(),
            max_tls_version: b.max_tls_version.as_deref(),
            ssl_cert_hostname: b.ssl_cert_hostname.as_deref(),
            ssl_sni_hostname: b.ssl_sni_hostname.as_deref(),
            request_condition: b.request_condition.as_deref(),
            weight: b.weight,
            auto_loadbalance: b.auto_loadbalance,
            shield: b.shield.as_deref(),
            healthcheck: b.healthcheck.as_deref(),
            connect_timeout: b.connect_timeout,
            first_byte_timeout: b.first_byte_timeout,
            between_bytes_timeout: b.between_bytes_timeout,
            max_conn: b.max_conn,
            created_at: b.created_at.as_deref(),
            updated_at: b.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim backend summaries for `service_id`@`version`.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message —
/// the same handling whether the service id or the version is unknown
/// (Fastly returns 404 in both cases and we don't try to disambiguate).
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceBackendsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let backends = match list_backends(
        &mut cfg,
        ListBackendsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(b) => b,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No backends found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_backends failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<BackendSummary> = backends.iter().map(BackendSummary::from_response).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
