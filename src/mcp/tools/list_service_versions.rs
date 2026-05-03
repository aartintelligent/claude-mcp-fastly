//! `list_service_versions` tool: list a Fastly service's active version
//! and the draft versions sitting above it.
//!
//! Unlike the other `list_service_*` tools, this one takes only
//! `service_id` (not `version`) — its purpose is to surface the current
//! production line plus any in-flight work that hasn't been deployed yet.
//!
//! Filter applied: `version_number >= active_version_number AND
//! (active OR not locked)`. That removes:
//!
//! - the historical lineage (older versions, all locked);
//! - versions left locked above active by a rollback (they were active
//!   at some point in the past but are no longer part of the workflow).
//!
//! What remains is the active version itself plus the unlocked draft
//! versions above it. When the service has no active version (e.g.,
//! brand-new, never deployed) the version-number filter is disabled but
//! the locked-only-when-not-active filter still applies.

use fastly_api::apis::Error;
use fastly_api::apis::version_api::{ListServiceVersionsParams, list_service_versions};
use fastly_api::models::VersionResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_versions` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceVersionsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
}

/// Slimmed-down view of a Fastly [`VersionResponse`].
///
/// Mirrors the fields documented at
/// <https://www.fastly.com/documentation/reference/api/services/version/>,
/// keeping only the operationally meaningful subset:
///
/// - `number` — the version's stable identifier within the service;
/// - `active` — whether this is the currently-served version;
/// - `locked` — whether the version is frozen (active and historical
///   versions are locked);
/// - `environments` — environment names where this version is deployed,
///   projected from the upstream `Vec<Environment>` to a `Vec<&str>` of
///   names (cross-references `Environment.active_version` upstream).
///
/// Drops `comment`, `deployed`, `staging`, `testing` (the latter three are
/// documented upstream as "Unused at this time"), `deleted_at`, and the
/// caller-known `service_id`.
#[derive(Serialize)]
struct VersionSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environments: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> VersionSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `v`.
    fn from_response(v: &'a VersionResponse) -> Self {
        Self {
            number: v.number,
            active: v.active,
            locked: v.locked,
            environments: v
                .environments
                .as_deref()
                .map(|envs| envs.iter().filter_map(|e| e.name.as_deref()).collect()),
            created_at: v.created_at.as_deref(),
            updated_at: v.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim version summaries for the service.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// for the unknown `service_id` case.
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceVersionsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let versions = match list_service_versions(
        &mut cfg,
        ListServiceVersionsParams {
            service_id: args.service_id.clone(),
        },
    )
    .await
    {
        Ok(v) => v,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No service found with id `{}`.",
                args.service_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_service_versions failed: {e}"),
                None,
            ));
        }
    };

    // Keep the active version and any unlocked drafts above it. We drop
    // versions older than the active (historical lineage) and versions
    // left locked above the active by a rollback (no longer part of the
    // workflow). The active version itself is locked but is preserved
    // explicitly via the `v.active == Some(true)` clause.
    let active_number = versions
        .iter()
        .find(|v| v.active == Some(true))
        .and_then(|v| v.number);

    let summaries: Vec<VersionSummary> = versions
        .iter()
        .filter(|v| {
            let above_active = match active_number {
                Some(cutoff) => v.number.is_some_and(|n| n >= cutoff),
                None => true,
            };
            let active_or_unlocked = v.active == Some(true) || v.locked != Some(true);
            above_active && active_or_unlocked
        })
        .map(VersionSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
