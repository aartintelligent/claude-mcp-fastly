//! `get_service_package` tool: fetch the Compute (wasm) package metadata
//! for a Fastly service version.
//!
//! Backed by `GET /service/{service_id}/version/{version_id}/package`.
//! This endpoint is meaningful only for services of type `wasm`
//! (Compute) — VCL services do not carry a package and the upstream
//! returns `404`. We downgrade that 404 to a plain-text "no package"
//! message so the agent gets a clear signal: either the service isn't a
//! Compute service, or no package has been uploaded yet.

use fastly_api::apis::Error;
use fastly_api::apis::package_api::{GetPackageParams, get_package};
use fastly_api::models::PackageResponse;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `get_service_package` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetServicePackageArgs {
    /// Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`).
    pub service_id: String,
    /// Service version number to inspect (typically the currently-active one,
    /// obtained via `get_service`).
    pub version: i32,
}

/// Slimmed-down view of a Fastly [`PackageResponse`].
///
/// Hoists the relevant `metadata` fields up to the top level so the agent
/// gets a flat, readable shape instead of a nested object. Drops the
/// caller-known context fields (`service_id`, `version`), `deleted_at`,
/// and the deprecated `hashsum` (replaced upstream by `files_hash`).
#[derive(Serialize)]
struct PackageSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authors: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    files_hash: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

impl<'a> PackageSummary<'a> {
    /// Builds a summary by borrowing the relevant slices of `pkg`. The
    /// nested `metadata` block is flattened: every interesting field is
    /// surfaced at the top level.
    fn from_response(pkg: &'a PackageResponse) -> Self {
        let metadata = pkg.metadata.as_deref();
        Self {
            id: pkg.id.as_deref(),
            name: metadata.and_then(|m| m.name.as_deref()),
            description: metadata.and_then(|m| m.description.as_deref()),
            language: metadata.and_then(|m| m.language.as_deref()),
            authors: metadata.and_then(|m| m.authors.as_deref()),
            size: metadata.and_then(|m| m.size),
            files_hash: metadata.and_then(|m| m.files_hash.as_deref()),
            created_at: pkg.created_at.as_deref(),
            updated_at: pkg.updated_at.as_deref(),
        }
    }
}

/// Returns a JSON object describing the package deployed at
/// `(service_id, version)`.
///
/// A `404` from Fastly is downgraded to a plain-text "no package" message
/// covering both: (a) the service is not a Compute service (VCL services
/// have no package), (b) the version has no package uploaded yet, and
/// (c) the service id or version is unknown.
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails for any reason
/// other than the 404 above (network, auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: GetServicePackageArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    match get_package(
        &mut cfg,
        GetPackageParams {
            service_id: args.service_id.clone(),
            version_id: args.version,
        },
    )
    .await
    {
        Ok(pkg) => {
            let summary = PackageSummary::from_response(&pkg);
            Ok(CallToolResult::success(vec![Content::json(&summary)?]))
        }
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "No Compute package found — service `{}` version `{}` either is not a Compute service, has no package uploaded, or does not exist.",
                args.service_id, args.version
            ))]))
        }
        Err(e) => Err(McpError::internal_error(
            format!("Fastly get_package failed: {e}"),
            None,
        )),
    }
}
