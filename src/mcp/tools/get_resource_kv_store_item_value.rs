//! `get_resource_kv_store_item_value` tool: fetch the value associated
//! with a single key in a Fastly KV store.
//!
//! Backed by `GET /resources/stores/kv/{store_id}/keys/{key}`. Fastly
//! returns the value as the **raw response body** (not JSON-encoded),
//! which the upstream `fastly_api::kv_store_item_api::kv_store_get_item`
//! SDK function fails to handle: it pipes the body through
//! `serde_json::from_str`, which only succeeds when the value happens to
//! be a JSON-encoded string. We bypass the SDK and issue a raw `GET`,
//! reusing [`AppState::fastly_config`]'s `reqwest::Client`, auth header,
//! and User-Agent — only the response decoding is replaced.
//!
//! The raw bytes are decoded as UTF-8 (replacing invalid sequences) and
//! returned to the agent as a JSON object `{ store_id, key, value }`.
//! Binary blobs that don't round-trip through UTF-8 will lose fidelity —
//! KV stores can technically hold arbitrary bytes, but the MCP transport
//! expects text-shaped content, so this trade-off is intentional.

use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `get_resource_kv_store_item_value` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetResourceKvStoreItemValueArgs {
    /// Alphanumeric Fastly KV store identifier — typically obtained from
    /// `list_resource_kv_stores`'s `id` field.
    pub store_id: String,
    /// The key to read. Listed by `list_resource_kv_store_items` (which
    /// returns keys only — values must be fetched one at a time via this
    /// tool).
    pub key: String,
}

/// Wrapper carrying the requested store/key alongside the value, so the
/// agent has the full context in a single JSON object.
#[derive(Serialize)]
struct KvStoreItemValue<'a> {
    store_id: &'a str,
    key: &'a str,
    value: String,
}

/// Returns the value at `(store_id, key)`.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// (covers both unknown store id and unknown key — Fastly does not
/// distinguish them at this endpoint).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, non-2xx status, body read failure).
pub async fn run(
    state: &AppState,
    args: GetResourceKvStoreItemValueArgs,
) -> Result<CallToolResult, McpError> {
    let cfg = state.fastly_config();

    // Mirror the path the SDK builds, with the same percent-encoding.
    let url = format!(
        "{}/resources/stores/kv/{}/keys/{}",
        cfg.base_path,
        urlencode(&args.store_id),
        urlencode(&args.key),
    );

    let mut req = cfg.client.get(&url);
    if let Some(api_key) = cfg.api_key.as_ref() {
        req = req.header("Fastly-Key", &api_key.key);
    }
    if let Some(ua) = cfg.user_agent.as_ref() {
        req = req.header("User-Agent", ua);
    }

    let resp = req.send().await.map_err(|e| {
        McpError::internal_error(format!("Fastly kv_store_get_item HTTP failed: {e}"), None)
    })?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "No value found for key `{}` in KV store `{}`.",
            args.key, args.store_id
        ))]));
    }
    if !status.is_success() {
        return Err(McpError::internal_error(
            format!("Fastly kv_store_get_item HTTP {status}"),
            None,
        ));
    }

    let value = resp.text().await.map_err(|e| {
        McpError::internal_error(
            format!("Fastly kv_store_get_item response read failed: {e}"),
            None,
        )
    })?;

    Ok(CallToolResult::success(vec![Content::json(
        &KvStoreItemValue {
            store_id: &args.store_id,
            key: &args.key,
            value,
        },
    )?]))
}

/// Percent-encodes a single path segment. Mirrors the SDK's helper
/// (`fastly_api::apis::urlencode`) without pulling in extra deps —
/// `form_urlencoded` is already part of the `url` crate that the
/// `reqwest`-based SDK transitively brings in, but keeping the dependency
/// surface narrow we just call its byte serializer.
fn urlencode(s: &str) -> String {
    fastly_api::apis::urlencode(s)
}
