//! `list_service_resources` tool: list the account-scoped resources
//! (KV / secret / config stores) linked to a specific Fastly service
//! version.
//!
//! This is the bridge between the service-version world and the
//! account-scoped store world: each entry returned is a *link*, not a
//! store. A link carries its own `id` and `name` (which may legitimately
//! differ from the underlying store's name) plus the `resource_id` of
//! the store itself — that `resource_id` is the input the agent feeds to
//! `list_resource_config_store_items`, `list_resource_kv_store_items`, or
//! `list_resource_secret_store_items` (chosen by `resource_type`).
//!
//! ACLs are intentionally absent from this surface: Compute ACLs are
//! linked through the dedicated Compute ACL API (`list_resource_acls`),
//! and VCL ACLs live inside the version's VCL config rather than as
//! external linked resources.

use fastly_api::apis::Error;
use fastly_api::apis::resource_api::{ListResourcesParams, list_resources};
use fastly_api::models::TypeResource;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_resources` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceResourcesArgs {
    /// Alphanumeric Fastly service identifier.
    pub service_id: String,
    /// Service version number — typically the active version returned by
    /// `get_service`.
    pub version: i32,
}

/// One resource-link summary. Mirrors the upstream `ResourceResponse`
/// shape minus context fields the caller already knows (`service_id`,
/// `version`) and operational noise (`deleted_at`).
#[derive(Serialize)]
struct ResourceLinkSummary<'a> {
    /// Identifier of the link itself (not the underlying store).
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    /// Human-readable name of the link. Note this can differ from the
    /// underlying store's name — Fastly treats them as independent.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    /// Identifier of the linked store. Feed this to the matching
    /// `list_resource_*_items` tool (`config` → config store,
    /// `kv-store` → KV store, `secret-store` → secret store).
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_id: Option<&'a str>,
    /// `"config"` | `"kv-store"` | `"secret-store"`. Drives which of the
    /// account-scoped tools to call next.
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_type: Option<&'a TypeResource>,
    /// Fastly API path to the underlying store, e.g.
    /// `/resources/stores/kv/{id}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    href: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Returns the resource links attached to `(service_id, version)`.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// (covers both unknown service id and unknown version).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceResourcesArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let resources = match list_resources(
        &mut cfg,
        ListResourcesParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No resources found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_resources failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<ResourceLinkSummary> = resources
        .iter()
        .map(|r| ResourceLinkSummary {
            id: r.id.as_deref(),
            name: r.name.as_deref(),
            resource_id: r.resource_id.as_deref(),
            resource_type: r.resource_type.as_ref(),
            href: r.href.as_deref(),
            created_at: r.created_at.as_deref(),
            updated_at: r.updated_at.as_deref(),
        })
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
