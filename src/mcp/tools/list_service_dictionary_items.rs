//! `list_service_dictionary_items` tool: fetch the key/value items of a
//! single Fastly edge dictionary.
//!
//! Backed by `GET /service/{service_id}/dictionary/{dictionary_id}/items`.
//! Items are *not* version-scoped — Fastly manages dictionary contents
//! out-of-band of versioned config, so this tool only takes
//! `(service_id, dictionary_id)`. The `dictionary_id` itself comes from
//! [`super::list_service_dictionaries`], which is the natural prior step.
//!
//! Optional `page` / `per_page` arguments are forwarded to Fastly for
//! pagination — useful when a dictionary has thousands of entries (we have
//! seen 1500+ in the wild) and the agent only wants a slice.
//!
//! Two upstream conditions get text-fallback handling:
//!
//! - `404 Not Found` (unknown service or dictionary id) → plain-text
//!   "not found" message.
//! - `403 Forbidden` (dictionary is `write_only: true`) → plain-text
//!   "write-only, items are not readable" so the agent learns *why*
//!   without parsing an MCP error.

use fastly_api::apis::Error;
use fastly_api::apis::dictionary_item_api::{ListDictionaryItemsParams, list_dictionary_items};
use fastly_api::models::DictionaryItemResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_dictionary_items` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceDictionaryItemsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Alphanumeric Fastly dictionary identifier — typically obtained from
    /// `list_service_dictionaries`'s `id` field.
    pub dictionary_id: String,
    /// Optional 1-based page number (Fastly pagination).
    #[serde(default)]
    pub page: Option<i32>,
    /// Optional page size. Fastly's default is small (100 items); raise it
    /// to fetch large dictionaries in fewer round-trips.
    #[serde(default)]
    pub per_page: Option<i32>,
}

/// One key/value entry of a Fastly edge dictionary.
#[derive(Serialize)]
struct DictionaryItemSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    item_key: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_value: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> DictionaryItemSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `i`.
    fn from_response(i: &'a DictionaryItemResponse) -> Self {
        Self {
            item_key: i.item_key.as_deref(),
            item_value: i.item_value.as_deref(),
            created_at: i.created_at.as_deref(),
            updated_at: i.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim item summaries for the given dictionary.
///
/// Special status handling:
/// - `404` → plain-text "not found" (unknown service/dictionary).
/// - `403` → plain-text "write-only" message (dictionary's items are
///   protected from reads).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListServiceDictionaryItemsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let items = match list_dictionary_items(
        &mut cfg,
        ListDictionaryItemsParams {
            service_id: args.service_id.clone(),
            dictionary_id: args.dictionary_id.clone(),
            page: args.page,
            per_page: args.per_page,
            ..Default::default()
        },
    )
    .await
    {
        Ok(items) => items,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No dictionary found — service `{}` does not exist or has no dictionary with id `{}`.",
                args.service_id, args.dictionary_id
            ))]));
        }
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 403 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Dictionary `{}` is write-only — its items are not readable via the API.",
                args.dictionary_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_dictionary_items failed: {e}"),
                None,
            ));
        }
    };

    let summaries: Vec<DictionaryItemSummary> = items
        .iter()
        .map(DictionaryItemSummary::from_response)
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
