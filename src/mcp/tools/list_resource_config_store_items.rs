//! `list_resource_config_store_items` tool: list the keys of a single
//! Fastly config store.
//!
//! Backed by `GET /resources/stores/config/{config_store_id}/items`.
//! Like the parent config-store catalog, items are **account-scoped** —
//! they exist independently of any service version, so this tool only
//! takes `config_store_id`. The id itself comes from
//! [`super::list_resource_config_stores`], which is the natural prior step.
//!
//! **Important contract**: this tool returns **keys only**, not key/value
//! pairs. The Fastly API does return both via `list_config_store_items`,
//! but mirroring the KV pattern (`list_resource_kv_store_items`) we strip
//! values here so an agent inspecting a store with thousands of entries
//! does not blow its context budget. To read a specific value, pass the
//! key to [`super::get_resource_config_store_item_value`].
//!
//! Config stores do not have a `write_only` flag — every store is
//! readable. The only special handling is therefore the `404` case
//! (unknown store id), which the MCP downgrades to a plain-text "not
//! found" message.

use fastly_api::apis::Error;
use fastly_api::apis::config_store_item_api::{
    ListConfigStoreItemsParams, list_config_store_items,
};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::AppState;

/// Arguments accepted by the `list_resource_config_store_items` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceConfigStoreItemsArgs {
    /// Alphanumeric Fastly config store identifier — typically obtained
    /// from `list_resource_config_stores`'s `id` field.
    pub config_store_id: String,
}

/// Returns a JSON array of keys for the given config store.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// — the only special-case handling needed since config stores have no
/// write-only flag.
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListResourceConfigStoreItemsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let items = match list_config_store_items(
        &mut cfg,
        ListConfigStoreItemsParams {
            config_store_id: args.config_store_id.clone(),
        },
    )
    .await
    {
        Ok(items) => items,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No config store found with id `{}`.",
                args.config_store_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_config_store_items failed: {e}"),
                None,
            ));
        }
    };

    let keys: Vec<&str> = items.iter().filter_map(|i| i.item_key.as_deref()).collect();

    Ok(CallToolResult::success(vec![Content::json(&keys)?]))
}
