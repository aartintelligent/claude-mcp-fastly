//! `list_service_dictionaries` tool: list a Fastly service version's edge
//! dictionaries together with the key/value items each one carries.
//!
//! The tool composes two Fastly endpoints into a single agent-facing call:
//!
//! - `GET /service/{id}/version/{ver}/dictionary` — lists the dictionaries
//!   declared in this version (each with `id`, `name`, `write_only`, …).
//! - `GET /service/{id}/dictionary/{dict_id}/items` — for each
//!   dictionary, lists the key/value entries.
//!
//! Items are nested inside their parent dictionary in the response so the
//! agent gets the full picture in one round-trip.
//!
//! Write-only dictionaries (`write_only: true`) hold values the API
//! refuses to read back (typical for secrets-style dictionaries). For
//! those, we omit the `items` field entirely instead of issuing a call
//! that would 4xx — the `write_only` flag itself is preserved so the
//! agent understands why items are absent.

use std::collections::HashMap;

use fastly_api::apis::Error;
use fastly_api::apis::dictionary_api::{ListDictionariesParams, list_dictionaries};
use fastly_api::apis::dictionary_item_api::{ListDictionaryItemsParams, list_dictionary_items};
use fastly_api::models::DictionaryItemResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_service_dictionaries` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceDictionariesArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
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

/// Slimmed-down view of a Fastly [`DictionaryResponse`], with its
/// key/value items nested under `items`.
///
/// `items` is `None` (and hence omitted) for write-only dictionaries — the
/// `write_only` flag explains why. For readable dictionaries, `items` is
/// `Some(_)` and may be empty.
#[derive(Serialize)]
struct DictionarySummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    write_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Vec<DictionaryItemSummary<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Returns a JSON array of dictionary summaries (with items nested) for
/// `service_id`@`version`.
///
/// A `404` from Fastly on the dictionaries-list call is downgraded to a
/// plain-text "not found" message, covering both unknown service id and
/// unknown version.
///
/// # Errors
///
/// Returns an MCP internal error if `list_dictionaries` fails for any
/// non-404 reason, or if any of the per-dictionary
/// `list_dictionary_items` follow-ups fails.
pub async fn run(
    state: &AppState,
    args: ListServiceDictionariesArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    // Phase 1: list dictionaries declared by this version.
    let dictionaries = match list_dictionaries(
        &mut cfg,
        ListDictionariesParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(d) => d,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No dictionaries found — service `{}` version `{}` does not exist.",
                args.service_id, args.version
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly list_dictionaries failed: {e}"),
                None,
            ));
        }
    };

    // Phase 2: for every readable (non-write-only) dictionary, pull its
    // items. Sequential — typical services carry only a handful of
    // dictionaries, so the parallelism gain isn't worth the JoinSet
    // ceremony.
    let mut items_by_id: HashMap<String, Vec<DictionaryItemResponse>> = HashMap::new();
    for d in &dictionaries {
        if d.write_only == Some(true) {
            continue;
        }
        let Some(did) = d.id.as_deref().map(String::clone) else {
            continue;
        };

        let items = list_dictionary_items(
            &mut cfg,
            ListDictionaryItemsParams {
                service_id: args.service_id.clone(),
                dictionary_id: did.clone(),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| {
            McpError::internal_error(
                format!("Fastly list_dictionary_items failed for `{did}`: {e}"),
                None,
            )
        })?;

        items_by_id.insert(did, items);
    }

    // Phase 3: compose the final response. Items live alongside their
    // parent dictionary; lookup goes through `items_by_id` keyed by the
    // dictionary's Fastly id.
    let summaries: Vec<DictionarySummary> = dictionaries
        .iter()
        .map(|d| {
            let id = d.id.as_deref().map(String::as_str);
            let items = if d.write_only == Some(true) {
                None
            } else {
                id.and_then(|i| items_by_id.get(i)).map(|items| {
                    items
                        .iter()
                        .map(DictionaryItemSummary::from_response)
                        .collect()
                })
            };
            DictionarySummary {
                id,
                name: d.name.as_deref(),
                write_only: d.write_only,
                items,
                created_at: d.created_at.as_deref(),
                updated_at: d.updated_at.as_deref(),
            }
        })
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
