//! `list_resource_config_stores` tool: list the Fastly account's config
//! stores, each enriched with its current `item_count`.
//!
//! Config stores are an **account-scoped** resource — they can be linked
//! to one or several services but exist independently of any single
//! service version. This tool therefore takes no `service_id` and no
//! `version`. It composes two Fastly endpoints into a single agent-facing
//! call:
//!
//! - `GET /resources/stores/config` — lists every config store in the
//!   account (optionally filtered by exact `name`).
//! - `GET /resources/stores/config/{id}/info` — for each store, returns
//!   the current item count.
//!
//! Items themselves are intentionally **not** included here; fetching
//! them is the job of a dedicated drill-down tool (to be added when
//! needed). Splitting the catalog from the items lets the agent triage
//! cheaply on item counts before deciding which store to expand.

use std::collections::HashMap;

use fastly_api::apis::config_store_api::{
    GetConfigStoreInfoParams, ListConfigStoresParams, get_config_store_info, list_config_stores,
};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_resource_config_stores` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceConfigStoresArgs {
    /// Optional exact-name filter forwarded to Fastly. When set, the
    /// upstream response is restricted to a single store (or empty).
    #[serde(default)]
    pub name: Option<String>,
}

/// Slimmed-down view of a Fastly config store, enriched with the
/// per-store info endpoint's `item_count`.
#[derive(Serialize)]
struct ConfigStoreSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Returns a JSON array of config-store summaries (with item count) for
/// the configured Fastly account.
///
/// # Errors
///
/// Returns an MCP internal error if `list_config_stores` fails for any
/// reason, or if any of the per-store `get_config_store_info` follow-ups
/// fails.
pub async fn run(
    state: &AppState,
    args: ListResourceConfigStoresArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    // Phase 1: list the account's config stores (optionally filtered by name).
    let stores = list_config_stores(
        &mut cfg,
        ListConfigStoresParams {
            name: args.name.clone(),
        },
    )
    .await
    .map_err(|e| {
        McpError::internal_error(format!("Fastly list_config_stores failed: {e}"), None)
    })?;

    // Phase 2: per-store info call to capture item counts. Sequential —
    // accounts typically carry only a handful of config stores, so the
    // simplicity wins over parallelism here.
    let mut count_by_id: HashMap<String, i32> = HashMap::new();
    for s in &stores {
        let Some(sid) = s.id.clone() else {
            continue;
        };

        let info = get_config_store_info(
            &mut cfg,
            GetConfigStoreInfoParams {
                config_store_id: sid.clone(),
            },
        )
        .await
        .map_err(|e| {
            McpError::internal_error(
                format!("Fastly get_config_store_info failed for `{sid}`: {e}"),
                None,
            )
        })?;

        if let Some(n) = info.item_count {
            count_by_id.insert(sid, n);
        }
    }

    // Phase 3: compose the final response.
    let summaries: Vec<ConfigStoreSummary> = stores
        .iter()
        .map(|s| {
            let id = s.id.as_deref();
            ConfigStoreSummary {
                id,
                name: s.name.as_deref(),
                item_count: id.and_then(|i| count_by_id.get(i).copied()),
                created_at: s.created_at.as_deref(),
                updated_at: s.updated_at.as_deref(),
            }
        })
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
