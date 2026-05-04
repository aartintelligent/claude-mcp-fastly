//! `list_service_dictionaries` tool: list a Fastly service version's edge
//! dictionaries with their per-dict info (item count, last updated,
//! content digest).
//!
//! The tool composes two Fastly endpoints into a single agent-facing call:
//!
//! - `GET /service/{id}/version/{ver}/dictionary` — lists the dictionaries
//!   declared in this version (each with `id`, `name`, `write_only`, …).
//! - `GET /service/{id}/version/{ver}/dictionary/{dict_id}/info` — for
//!   each dictionary, fetches its metadata (item count, content digest,
//!   last update timestamp).
//!
//! Items themselves are intentionally **not** included here — fetching
//! them is the job of [`super::list_service_dictionary_items`] (separate
//! tool). Splitting the listing from the items lets the agent triage
//! cheaply on item counts before deciding which dictionaries to expand.
//!
//! `get_dictionary_info` works on `write_only` dictionaries too (only the
//! item *values* are forbidden, the count is not), so we surface
//! `item_count` for every dictionary regardless of its readability flag.

use std::collections::HashMap;

use fastly_api::apis::Error;
use fastly_api::apis::dictionary_api::{ListDictionariesParams, list_dictionaries};
use fastly_api::apis::dictionary_info_api::{GetDictionaryInfoParams, get_dictionary_info};
use fastly_api::models::DictionaryInfoResponse;
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

/// Slimmed-down view of a Fastly dictionary, enriched with the per-dict
/// info endpoint.
///
/// `item_count`, `digest`, and `last_updated` come from
/// `get_dictionary_info`. They are filled in for every dictionary
/// (including write-only ones, where the count is still readable) and
/// omitted via `skip_serializing_if` when the upstream returned `None`
/// for them.
#[derive(Serialize)]
struct DictionarySummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    write_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_updated: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Returns a JSON array of dictionary summaries for `service_id`@`version`,
/// each enriched with item count, content digest, and last-updated
/// timestamp.
///
/// A `404` from Fastly on the dictionaries-list call is downgraded to a
/// plain-text "not found" message, covering both unknown service id and
/// unknown version. Per-dictionary info failures are propagated as MCP
/// internal errors (a single broken dictionary fails the whole call —
/// signal-over-noise).
///
/// # Errors
///
/// Returns an MCP internal error if `list_dictionaries` fails for any
/// non-404 reason, or if any of the per-dictionary `get_dictionary_info`
/// follow-ups fails.
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

    // Phase 2: for every dictionary, pull info (item_count, digest,
    // last_updated). Sequential — typical services carry only a handful
    // of dictionaries, and each call is a cheap metadata lookup. We
    // include write-only dictionaries: `get_dictionary_info` returns
    // their counts even though `list_dictionary_items` would refuse them.
    let mut info_by_id: HashMap<String, DictionaryInfoResponse> = HashMap::new();
    for d in &dictionaries {
        let Some(did) = d.id.as_deref().cloned() else {
            continue;
        };

        let info = get_dictionary_info(
            &mut cfg,
            GetDictionaryInfoParams {
                service_id: args.service_id.clone(),
                version_id: args.version,
                dictionary_id: did.clone(),
            },
        )
        .await
        .map_err(|e| {
            McpError::internal_error(
                format!("Fastly get_dictionary_info failed for `{did}`: {e}"),
                None,
            )
        })?;

        info_by_id.insert(did, info);
    }

    // Phase 3: compose the final response.
    let summaries: Vec<DictionarySummary> = dictionaries
        .iter()
        .map(|d| {
            let id = d.id.as_deref().map(String::as_str);
            let info = id.and_then(|i| info_by_id.get(i));
            DictionarySummary {
                id,
                name: d.name.as_deref(),
                write_only: d.write_only,
                item_count: info.and_then(|i| i.item_count),
                digest: info.and_then(|i| i.digest.as_deref()),
                last_updated: info.and_then(|i| i.last_updated.as_deref()),
                created_at: d.created_at.as_deref(),
                updated_at: d.updated_at.as_deref(),
            }
        })
        .collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
