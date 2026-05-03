//! `list_service_vcl_conditions` tool: list the conditions of a specific
//! Fastly VCL service version (`service_id` + `version`).
//!
//! Conditions are named VCL boolean expressions that other configuration
//! objects (cache settings, headers, request settings, response objects,
//! …) reference to gate their behavior — e.g. *"apply this header rewrite
//! only when `req.http.User-Agent ~ 'Bot'`"*. This is a VCL-service feature
//! — hence the `_vcl_` segment in the tool name — it does not exist on
//! Compute services. Same contract as the other `list_service_*` tools.

use fastly_api::apis::Error;
use fastly_api::apis::condition_api::{ListConditionsParams, list_conditions};
use fastly_api::models::ConditionResponse;
use fastly_api::models::condition_response::Type as ConditionType;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_vcl_conditions` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVclConditionsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`ConditionResponse`].
///
/// Mirrors the configurable fields documented at
/// <https://www.fastly.com/documentation/reference/api/vcl-services/condition/>,
/// plus the creation/update timestamps. Drops `comment`, `deleted_at`, and
/// the caller-known context fields (`service_id`, `version`).
///
/// Note: Fastly returns `priority` as a numeric string (e.g. `"100"`) — we
/// forward it as-is.
#[derive(Serialize)]
struct ConditionSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    condition_type: Option<&'a ConditionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statement: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> ConditionSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `c`.
    fn from_response(c: &'a ConditionResponse) -> Self {
        Self {
            name: c.name.as_deref(),
            condition_type: c._type.as_ref(),
            statement: c.statement.as_deref(),
            priority: c.priority.as_deref(),
            created_at: c.created_at.as_deref(),
            updated_at: c.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim condition summaries for
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
    args: ListServiceVclConditionsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let conditions = match list_conditions(
        &mut cfg,
        ListConditionsParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(c) => c,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No conditions found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_conditions failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<ConditionSummary> = conditions
        .iter()
        .map(ConditionSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
