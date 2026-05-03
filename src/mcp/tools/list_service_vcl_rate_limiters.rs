//! `list_service_vcl_rate_limiters` tool: list the rate limiters of a
//! specific Fastly VCL service version (`service_id` + `version`).
//!
//! A rate limiter watches incoming requests and triggers an action when a
//! given client (identified by VCL variables in `client_key`) exceeds an
//! RPS threshold over a sliding window. This is a VCL-service feature —
//! hence the `_vcl_` segment in the tool name — Compute services
//! implement rate limiting in user code instead. Same contract as the
//! other `list_service_*` tools.

use std::collections::{HashMap, HashSet};

use fastly_api::apis::Error;
use fastly_api::apis::rate_limiter_api::{ListRateLimitersParams, list_rate_limiters};
use fastly_api::models::RateLimiterResponse;
use fastly_api::models::rate_limiter_response;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_rate_limiters` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclRateLimitersArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`RateLimiterResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/rate-limiter/>,
/// plus the rate-limiter `id` and the creation/update timestamps. Drops
/// `feature_revision` (internal Fastly bookkeeping), `deleted_at`, and the
/// caller-known context fields (`service_id`, `version`).
///
/// Fields like `response`, `response_object_name`, and `logger_type` are
/// only meaningful for specific values of `action`; they are omitted via
/// `skip_serializing_if` when not populated.
#[derive(Serialize)]
struct RateLimiterSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri_dictionary_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_methods: Option<&'a HashSet<rate_limiter_response::HttpMethods>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rps_limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_size: Option<&'a rate_limiter_response::WindowSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_key: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    penalty_box_duration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<&'a rate_limiter_response::Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<&'a HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_object_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logger_type: Option<&'a rate_limiter_response::LoggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> RateLimiterSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `r`.
    fn from_response(r: &'a RateLimiterResponse) -> Self {
        Self {
            id: r.id.as_deref(),
            name: r.name.as_deref(),
            uri_dictionary_name: r.uri_dictionary_name.as_deref(),
            http_methods: r.http_methods.as_ref(),
            rps_limit: r.rps_limit,
            window_size: r.window_size.as_ref(),
            client_key: r.client_key.as_deref(),
            penalty_box_duration: r.penalty_box_duration,
            action: r.action.as_ref(),
            response: r.response.as_ref(),
            response_object_name: r.response_object_name.as_deref(),
            logger_type: r.logger_type.as_ref(),
            created_at: r.created_at.as_deref(),
            updated_at: r.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim rate-limiter summaries for
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
    args: ListServiceVclRateLimitersArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let rate_limiters = match list_rate_limiters(
        &mut cfg,
        ListRateLimitersParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No rate limiters found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_rate_limiters failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<RateLimiterSummary> = rate_limiters
        .iter()
        .map(RateLimiterSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
