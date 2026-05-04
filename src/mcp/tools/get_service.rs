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
//!
//! In addition, when the service has an active version, this tool returns
//! a `dependencies` map counting every config object attached to that
//! version (backends, directors, domains, healthchecks, plus the VCL-only
//! object types when the service is VCL). Counts are fetched by firing one
//! `list_*` Fastly call per type, in parallel via [`tokio::try_join!`] —
//! Fastly does not expose a direct count endpoint, so the page-and-count
//! pattern is the only option, and parallelizing keeps the wall-clock
//! latency to a single network round (per batch).

use fastly_api::apis::{
    Error,
    apex_redirect_api::{ListApexRedirectsParams, list_apex_redirects},
    backend_api::{ListBackendsParams, list_backends},
    cache_settings_api::{ListCacheSettingsParams, list_cache_settings},
    condition_api::{ListConditionsParams, list_conditions},
    domain_api::{ListDomainsParams, list_domains},
    gzip_api::{ListGzipConfigsParams, list_gzip_configs},
    header_api::{ListHeaderObjectsParams, list_header_objects},
    healthcheck_api::{ListHealthchecksParams, list_healthchecks},
    rate_limiter_api::{ListRateLimitersParams, list_rate_limiters},
    request_settings_api::{ListRequestSettingsParams, list_request_settings},
    response_object_api::{ListResponseObjectsParams, list_response_objects},
    service_api::{GetServiceParams, get_service},
    snippet_api::{ListSnippetsParams, list_snippets},
};
use fastly_api::models::ServiceResponse;
use fastly_api::models::service_response::Type;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::list_service_directors::fetch_directors_raw;
use crate::app::AppState;

/// Arguments accepted by the `get_service` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetServiceArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
}

/// Counts directors via [`fetch_directors_raw`], which bypasses the SDK's
/// broken `Vec<Backend>` deserialization. A `404` from Fastly during the
/// directors fetch is unexpected here — we already confirmed the service
/// exists via `get_service` before calling this — so it is reported as
/// an internal error rather than a benign empty count.
///
/// # Errors
///
/// Returns an MCP internal error on transport failure, non-success HTTP
/// status, response-body parse failure, or unexpected `404`.
async fn count_directors(
    state: &AppState,
    service_id: &str,
    version: i32,
) -> Result<usize, McpError> {
    fetch_directors_raw(state, service_id, version)
        .await?
        .map(|v| v.len())
        .ok_or_else(|| {
            McpError::internal_error(
                "Fastly list_directors returned 404 (service or version vanished mid-call)",
                None,
            )
        })
}

/// Builds an `async` future that fires a Fastly `list_*` call for
/// `(service_id, version)` and resolves to the count of items returned.
///
/// Each invocation clones the [`AppState`]'s Fastly client configuration
/// (cheap — the inner `reqwest::Client` is `Arc`-shared) so the resulting
/// future owns its own `Configuration` and can be `tokio::try_join!`'d in
/// parallel with the others without fighting `&mut Configuration` borrows.
macro_rules! count_dep {
    (
        $state:expr,
        $sid:expr,
        $version:expr,
        $list_fn:ident,
        $params_ty:ident,
        $label:literal
    ) => {{
        let mut cfg = $state.fastly_config();
        let sid: String = $sid.to_owned();
        let ver: i32 = $version;
        async move {
            $list_fn(
                &mut cfg,
                $params_ty {
                    service_id: sid,
                    version_id: ver,
                },
            )
            .await
            .map(|v| v.len())
            .map_err(|e| {
                McpError::internal_error(
                    format!(concat!("Fastly ", $label, " failed: {}"), e),
                    None,
                )
            })
        }
    }};
}

/// Counts of every config object attached to a service version.
///
/// The first four fields (`backends`, `directors`, `domains`,
/// `healthchecks`) apply to every service kind. The `vcl_*` fields are
/// populated only when the service has `type: "vcl"` — for Compute
/// services they are `None` and serialization omits them entirely thanks
/// to `skip_serializing_if`.
#[derive(Serialize)]
struct Dependencies {
    backends: usize,
    directors: usize,
    domains: usize,
    healthchecks: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_apex_redirects: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_cache_settings: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_conditions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_gzip: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_headers: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_rate_limiters: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_request_settings: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_response_objects: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vcl_snippets: Option<usize>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    dependencies: Option<Dependencies>,
}

impl<'a> ServiceSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `service`.
    /// `dependencies` is left as `None`; populate it afterwards from
    /// [`fetch_dependencies`].
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
            dependencies: None,
        }
    }
}

/// Fires every `list_*` Fastly call applicable to the service kind in
/// parallel and collects the resulting counts.
///
/// Two batches:
/// 1. Multi-kind (always): `backends`, `directors`, `domains`,
///    `healthchecks`.
/// 2. VCL-only (when `is_vcl`): the 9 `list_service_vcl_*` resources.
///
/// Each batch runs concurrently under `tokio::try_join!`, so the total
/// wall-clock latency is bounded by the slowest single call per batch
/// rather than by their sum.
async fn fetch_dependencies(
    state: &AppState,
    service_id: &str,
    version: i32,
    is_vcl: bool,
) -> Result<Dependencies, McpError> {
    let backends_fut = count_dep!(
        state,
        service_id,
        version,
        list_backends,
        ListBackendsParams,
        "list_backends"
    );
    let directors_fut = count_directors(state, service_id, version);
    let domains_fut = count_dep!(
        state,
        service_id,
        version,
        list_domains,
        ListDomainsParams,
        "list_domains"
    );
    let healthchecks_fut = count_dep!(
        state,
        service_id,
        version,
        list_healthchecks,
        ListHealthchecksParams,
        "list_healthchecks"
    );

    let (backends, directors, domains, healthchecks) =
        tokio::try_join!(backends_fut, directors_fut, domains_fut, healthchecks_fut,)?;

    if !is_vcl {
        return Ok(Dependencies {
            backends,
            directors,
            domains,
            healthchecks,
            vcl_apex_redirects: None,
            vcl_cache_settings: None,
            vcl_conditions: None,
            vcl_gzip: None,
            vcl_headers: None,
            vcl_rate_limiters: None,
            vcl_request_settings: None,
            vcl_response_objects: None,
            vcl_snippets: None,
        });
    }

    let apex_fut = count_dep!(
        state,
        service_id,
        version,
        list_apex_redirects,
        ListApexRedirectsParams,
        "list_apex_redirects"
    );
    let cache_fut = count_dep!(
        state,
        service_id,
        version,
        list_cache_settings,
        ListCacheSettingsParams,
        "list_cache_settings"
    );
    let conditions_fut = count_dep!(
        state,
        service_id,
        version,
        list_conditions,
        ListConditionsParams,
        "list_conditions"
    );
    let gzip_fut = count_dep!(
        state,
        service_id,
        version,
        list_gzip_configs,
        ListGzipConfigsParams,
        "list_gzip_configs"
    );
    let headers_fut = count_dep!(
        state,
        service_id,
        version,
        list_header_objects,
        ListHeaderObjectsParams,
        "list_header_objects"
    );
    let rate_limiters_fut = count_dep!(
        state,
        service_id,
        version,
        list_rate_limiters,
        ListRateLimitersParams,
        "list_rate_limiters"
    );
    let request_settings_fut = count_dep!(
        state,
        service_id,
        version,
        list_request_settings,
        ListRequestSettingsParams,
        "list_request_settings"
    );
    let response_objects_fut = count_dep!(
        state,
        service_id,
        version,
        list_response_objects,
        ListResponseObjectsParams,
        "list_response_objects"
    );
    let snippets_fut = count_dep!(
        state,
        service_id,
        version,
        list_snippets,
        ListSnippetsParams,
        "list_snippets"
    );

    let (
        apex,
        cache,
        conditions,
        gzip,
        headers,
        rate_limiters,
        request_settings,
        response_objects,
        snippets,
    ) = tokio::try_join!(
        apex_fut,
        cache_fut,
        conditions_fut,
        gzip_fut,
        headers_fut,
        rate_limiters_fut,
        request_settings_fut,
        response_objects_fut,
        snippets_fut,
    )?;

    Ok(Dependencies {
        backends,
        directors,
        domains,
        healthchecks,
        vcl_apex_redirects: Some(apex),
        vcl_cache_settings: Some(cache),
        vcl_conditions: Some(conditions),
        vcl_gzip: Some(gzip),
        vcl_headers: Some(headers),
        vcl_rate_limiters: Some(rate_limiters),
        vcl_request_settings: Some(request_settings),
        vcl_response_objects: Some(response_objects),
        vcl_snippets: Some(snippets),
    })
}

/// Fetches a Fastly service by id via the `service/{service_id}` endpoint,
/// then enriches the response with a `dependencies` count map for the
/// currently-active version.
///
/// A `404` from Fastly on the initial `get_service` call is downgraded to a
/// plain-text "no match" success result.
///
/// # Errors
///
/// Returns an MCP internal error if the upstream Fastly call fails for any
/// reason other than the 404 above (network, auth, deserialization, 5xx),
/// or if any of the `list_*` calls used to build `dependencies` fails.
pub async fn run(state: &AppState, args: GetServiceArgs) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let params = GetServiceParams {
        service_id: args.service_id.clone(),
    };

    let service = match get_service(&mut cfg, params).await {
        Ok(s) => s,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No service found with id `{}`.",
                args.service_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly get_service failed: {e}"),
                None,
            ));
        }
    };

    let mut summary = ServiceSummary::from_response(&service);

    if let Some(version) = summary.version {
        let is_vcl = matches!(service._type, Some(Type::Vcl));
        summary.dependencies =
            Some(fetch_dependencies(state, &args.service_id, version, is_vcl).await?);
    }

    Ok(CallToolResult::success(vec![Content::json(&summary)?]))
}
