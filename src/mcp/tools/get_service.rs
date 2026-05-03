//! `get_service` tool: fetch a Fastly service by its `service_id`.
//!
//! Backed by the `GET /service/{service_id}` endpoint, which returns the
//! service entry or a `404` if the id is unknown. The 404 is mapped to a
//! "no match" text response so the agent gets a clean answer instead of an
//! MCP-level error.
//!
//! The full upstream payload embeds *every* version of the service — often
//! dozens, each with their own metadata — which dwarfs the actually useful
//! information. We project it onto a [`ServiceSummary`] that keeps the
//! top-level service fields and surfaces only the currently-active version
//! number under [`ServiceSummary::version`].

use fastly_api::apis::Error;
use fastly_api::apis::service_api::{GetServiceParams, get_service};
use fastly_api::models::service_response::Type;
use fastly_api::models::ServiceResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `get_service` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetServiceArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
}

/// Slimmed-down view of a Fastly [`ServiceResponse`].
///
/// Borrows from the upstream payload to keep the projection zero-copy. The
/// `versions[]` array is intentionally dropped — only the active version
/// number is exposed via [`ServiceSummary::version`] — because the raw list
/// grows linearly with every deploy and quickly dominates the LLM context.
#[derive(Serialize)]
struct ServiceSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    service_type: Option<&'a Type>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> ServiceSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `service`.
    fn from_response(service: &'a ServiceResponse) -> Self {
        let version = service
            .versions
            .as_deref()
            .and_then(|v| v.iter().find(|ver| ver.active == Some(true)))
            .and_then(|ver| ver.number);

        Self {
            id: service.id.as_deref().map(String::as_str),
            name: service.name.as_deref(),
            service_type: service._type.as_ref(),
            version,
            created_at: service.created_at.as_deref(),
            updated_at: service.updated_at.as_deref(),
        }
    }
}

/// Fetches a Fastly service by id via the `service/{service_id}` endpoint.
///
/// Returns the service entry projected through [`ServiceSummary`] as JSON
/// content. A `404` from Fastly is downgraded to a plain-text "no match"
/// success result.
///
/// # Errors
///
/// Returns an MCP internal error if the upstream Fastly call fails for any
/// reason other than a 404 (network, auth, deserialization, 5xx).
pub async fn run(state: &AppState, args: GetServiceArgs) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let params = GetServiceParams {
        service_id: args.service_id.clone(),
    };

    match get_service(&mut cfg, params).await {
        Ok(service) => {
            let summary = ServiceSummary::from_response(&service);
            Ok(CallToolResult::success(vec![Content::json(&summary)?]))
        }
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "No service found with id `{}`.",
                args.service_id
            ))]))
        }
        Err(e) => Err(McpError::internal_error(
            format!("Fastly get_service failed: {e}"),
            None,
        )),
    }
}
