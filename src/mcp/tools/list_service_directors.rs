//! `list_service_directors` tool: list the directors of a specific Fastly
//! service version (`service_id` + `version`).
//!
//! A director is a load-balancing group that bundles several backends
//! together with a balancing strategy (random, hash, client-sticky), an
//! up-quorum threshold, optional origin shielding, and a retry budget.
//! Fastly catalogs directors under "load balancing" rather than
//! "vcl-services" — they are accessible on every service kind, though
//! they only carry runtime meaning for VCL services. Same contract as the
//! other `list_service_*` tools.
//!
//! # Why this file holds raw HTTP plumbing
//!
//! The `fastly_api` Rust SDK auto-generated from Fastly's `OpenAPI` spec
//! mismodels the directors response. Specifically:
//!
//! ```text
//! // fastly_api::models::director_response::DirectorResponse
//! pub backends: Option<Vec<crate::models::Backend>>,   // claims nested objects
//! ```
//!
//! …whereas the live API actually returns the backend list as plain name
//! strings:
//!
//! ```json
//! { "name": "lb-eu", "backends": ["origin-1", "origin-2"], ... }
//! ```
//!
//! As soon as a director carries at least one backend, `list_directors`
//! from the SDK panics during deserialization with:
//!
//! > *invalid type: string "origin-1", expected struct Backend*
//!
//! Until Fastly fixes the spec, we bypass the SDK on this single endpoint.
//! [`fetch_directors_raw`] reuses the SDK's `reqwest::Client` and
//! credentials (so connection pool, `Fastly-Key`, and User-Agent are
//! identical to every other Fastly call) and parses the response into
//! [`DirectorRaw`] — a struct that mirrors the *real* wire shape, with
//! `backends: Option<Vec<String>>`.
//!
//! [`crate::mcp::tools::get_service`] reuses [`fetch_directors_raw`] for
//! its `dependencies.directors` count, so this is the only place in the
//! crate that ever talks directly to the directors endpoint.

use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Director shape as actually returned by Fastly's
/// `GET /service/{id}/version/{ver}/director` endpoint.
///
/// Every field is `Option` so unset attributes simply disappear from the
/// payload. The crucial fix vs the SDK's `DirectorResponse` is
/// [`DirectorRaw::backends`] being `Option<Vec<String>>`, not
/// `Option<Vec<Backend>>` — see the module-level doc for context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct DirectorRaw {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub director_type: Option<DirectorType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quorum: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shield: Option<String>,
    /// Backend names attached to this director. Documented upstream as
    /// nested objects but actually returned as a string array — that's the
    /// shape the SDK gets wrong and we fix here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
}

/// Director's `type` field — accepts either an integer (`1`) or its string
/// form (`"1"`). Fastly's published examples and the OpenAPI-derived SDK
/// disagree on this point; an untagged enum lets us absorb either shape
/// without a deserialization failure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum DirectorType {
    Numeric(i32),
    Named(String),
}

/// Issues `GET /service/{service_id}/version/{version}/director` and
/// deserializes the response array into [`DirectorRaw`]s.
///
/// Reuses the SDK's `reqwest::Client` and credentials carried by
/// [`AppState::fastly_config`], so the connection pool, auth header, and
/// User-Agent stay aligned with all the other Fastly calls in this crate.
/// Only the typed deserialization is replaced.
///
/// # Returns
///
/// - `Ok(Some(directors))` on a successful response (possibly empty).
/// - `Ok(None)` on `404` — the caller decides whether to surface this as
///   "service/version not found" text or as an internal error.
/// - `Err(_)` for any other transport, status, or deserialization failure.
///
/// # Errors
///
/// Returns an MCP internal error if the HTTP call fails, the status is
/// neither 2xx nor 404, or the response body cannot be parsed.
pub(super) async fn fetch_directors_raw(
    state: &AppState,
    service_id: &str,
    version: i32,
) -> Result<Option<Vec<DirectorRaw>>, McpError> {
    let cfg = state.fastly_config();

    let url = format!(
        "{}/service/{}/version/{}/director",
        cfg.base_path, service_id, version,
    );

    let mut req = cfg.client.get(&url);
    if let Some(api_key) = cfg.api_key.as_ref() {
        req = req.header("Fastly-Key", &api_key.key);
    }
    if let Some(ua) = cfg.user_agent.as_ref() {
        req = req.header("User-Agent", ua);
    }

    let resp = req.send().await.map_err(|e| {
        McpError::internal_error(format!("Fastly list_directors HTTP failed: {e}"), None)
    })?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(McpError::internal_error(
            format!("Fastly list_directors HTTP {status}"),
            None,
        ));
    }

    let directors: Vec<DirectorRaw> = resp.json().await.map_err(|e| {
        McpError::internal_error(
            format!("Fastly list_directors response parse failed: {e}"),
            None,
        )
    })?;

    Ok(Some(directors))
}

/// Arguments accepted by the `list_service_directors` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListServiceDirectorsArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a [`DirectorRaw`].
///
/// Keeps the operationally meaningful fields (name, type, quorum, retries,
/// shield, backend names, timestamps) and drops `comment`, `capacity`
/// (documented upstream as unused), `deleted_at`, and the caller-known
/// context fields (`service_id`, `version`).
#[derive(Serialize)]
struct DirectorSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    director_type: Option<&'a DirectorType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quorum: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retries: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shield: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backends: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> DirectorSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `d`.
    fn from_raw(d: &'a DirectorRaw) -> Self {
        Self {
            name: d.name.as_deref(),
            director_type: d.director_type.as_ref(),
            quorum: d.quorum,
            retries: d.retries,
            shield: d.shield.as_deref(),
            backends: d.backends.as_deref(),
            created_at: d.created_at.as_deref(),
            updated_at: d.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON array of slim director summaries for
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
    args: ListServiceDirectorsArgs,
) -> Result<CallToolResult, McpError> {
    let Some(directors) = fetch_directors_raw(state, &args.service_id, args.version).await? else {
        return Ok(CallToolResult::success(vec![Content::text(format!(
            "No directors found — service `{}` version `{}` does not exist.",
            args.service_id, args.version
        ))]));
    };

    let summaries: Vec<DirectorSummary> = directors.iter().map(DirectorSummary::from_raw).collect();

    Ok(CallToolResult::success(vec![Content::json(&summaries)?]))
}
