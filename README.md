## About

### claude-mcp-fastly MCP Server

Connects an LLM agent to Fastly's management API through 30 focused tools. Designed for inspection and audit workflows, not for provisioning.

### What is an MCP Server?

The Model Context Protocol (MCP) is an open protocol for connecting LLM clients (Claude Desktop, Claude Code, Cursor, etc.) to external tools and data sources. An MCP server exposes a set of tools that a client can call on behalf of an LLM, with structured arguments and structured responses. This server speaks MCP over the streamable-HTTP transport and exposes Fastly inspection tools.

---

## MCP Info

| Attribute | Value |
|---|---|
| Docker Image | `aartintelligent/claude-mcp-fastly:latest` |
| Author | Aurelien Andre |
| Repository | https://github.com/aartintelligent/claude-mcp-fastly |
| License | MIT |
| MCP Protocol | `2025-11-25` |
| Transport | Streamable HTTP (endpoint: `/mcp`) |
| Mode | Read-only |
| Requires Secrets | Yes — Fastly API token |

---

## Image Building Info

| Attribute | Value |
|---|---|
| Dockerfile | [`Dockerfile`](./Dockerfile) |
| Build base | `dhi.io/rust:1.95-debian13-dev` (Docker Hardened Image) |
| Runtime base | `dhi.io/debian-base:trixie-debian13` (Docker Hardened Image) |
| Language | Rust 2024 edition, MSRV 1.95.0 |
| HTTP framework | `axum` |
| MCP framework | `rmcp` |
| Fastly client | `fastly-api 13.x` (official Fastly Rust SDK) |
| Runtime user | `nonroot` (DHI default) |
| Default port | `8000` |

---

## Available Tools (30)

| Tool | Description |
|---|---|
| `find_domain` | Find a domain in the Fastly account by FQDN. |
| `find_resource_acl_entry` | Find the Compute ACL entry (CIDR + action) that covers an IP. |
| `get_service` | Get a Fastly service's metadata and active version number. |
| `get_service_package` | Get a Fastly Compute (wasm) service version's package metadata. |
| `get_resource_config_store_item_value` | Get the value of a single key in a config store. |
| `get_resource_kv_store_item_value` | Get the value of a single key in a KV store. |
| `list_resource_acls` | List the Fastly account's Compute ACLs (catalog only). |
| `list_resource_config_stores` | List the Fastly account's config stores with item counts. |
| `list_resource_config_store_items` | List the keys of a config store. |
| `list_resource_kv_stores` | List the Fastly account's KV stores. |
| `list_resource_kv_store_items` | List the keys of a KV store (cursor-paginated, optional prefix). |
| `list_resource_secret_stores` | List the Fastly account's secret stores. |
| `list_resource_secret_store_items` | List the secrets of a secret store (names + digests only). |
| `list_service_versions` | List a Fastly service's active version and any draft versions above it. |
| `list_service_backends` | List a Fastly service version's origin backends. |
| `list_service_dictionaries` | List a Fastly service version's edge dictionaries (with item count, digest, last-updated). |
| `list_service_dictionary_items` | List the key/value items of a Fastly edge dictionary. |
| `list_service_directors` | List a Fastly service version's directors (load-balancing groups of backends). |
| `list_service_domains` | List a Fastly service version's domains. |
| `list_service_healthchecks` | List a Fastly service version's healthcheck probes. |
| `list_service_resources` | List the account-scoped stores (KV / secret / config) linked to a service version. |
| `list_service_vcl_apex_redirects` | List a Fastly VCL service version's apex redirects. |
| `list_service_vcl_cache_settings` | List a Fastly VCL service version's cache-settings rules. |
| `list_service_vcl_conditions` | List a Fastly VCL service version's conditions (named VCL boolean expressions). |
| `list_service_vcl_gzip` | List a Fastly VCL service version's gzip compression configurations. |
| `list_service_vcl_headers` | List a Fastly VCL service version's header rules (set/append/delete/regex). |
| `list_service_vcl_rate_limiters` | List a Fastly VCL service version's rate limiters. |
| `list_service_vcl_request_settings` | List a Fastly VCL service version's request-settings rules. |
| `list_service_vcl_response_objects` | List a Fastly VCL service version's response objects (canned HTTP responses). |
| `list_service_vcl_snippets` | List a Fastly VCL service version's VCL snippets (code injected into a specific request-lifecycle phase). |

The tools fall into five groups:

- **Entry points** (no `version` argument): `find_domain`, `get_service`, `list_service_versions`.
- **Account-scoped resources** (no `service_id` / `version`): `list_resource_acls` + `find_resource_acl_entry`; `list_resource_config_stores` → `list_resource_config_store_items` → `get_resource_config_store_item_value`; `list_resource_kv_stores` → `list_resource_kv_store_items` → `get_resource_kv_store_item_value`; `list_resource_secret_stores` → `list_resource_secret_store_items` (Fastly never returns secret values, so there is intentionally no per-value tool).
- **Multi-kind service tools** (work on every service `type`): `list_service_backends`, `list_service_directors`, `list_service_domains`, `list_service_healthchecks`, `list_service_dictionaries` → `list_service_dictionary_items`, `list_service_resources`.
- **Compute-only** (require `type: "wasm"`): `get_service_package`.
- **VCL-only** (require `type: "vcl"`): the nine `list_service_vcl_*` tools.

Cross-references between tools:

- A backend's `healthcheck` field is the `name` of a `list_service_healthchecks` entry.
- A director's `backends` array contains the `name`s of `list_service_backends` entries.
- Cache settings, headers, request/response settings, and rate limiters reference VCL conditions by `name` via their `*_condition` fields → chain into `list_service_vcl_conditions`.
- VCL snippets often reference edge dictionaries via `table.lookup(<dict>, "<key>")` → chain into `list_service_dictionaries` and `list_service_dictionary_items` to see the actual values.
- `list_service_resources[].resource_id` + `.resource_type` (`config` / `kv-store` / `secret-store`) drives the next call to the matching account-scoped `list_resource_*_items` tool.
- Compute ACLs are referenced by `id` from Compute services; resolve them with `list_resource_acls` and probe specific IPs with `find_resource_acl_entry` (Fastly's dedicated lookup endpoint, no scan).
- 404s from Fastly (unknown service / version / store / key / ACL) are downgraded to plain-text messages — treat as a clean empty signal, not a failure to retry.

---

## Tools Details

### Tool: `find_domain`

Look up a domain in the account's Domain Management v1 catalog by FQDN. Returns the domain id, FQDN, associated `service_id` (when bound), and TLS activation/verification flags.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `fqdn` | string | yes | Fully-qualified domain name to look up (e.g. `www.example.com`). |
| `fqdn_match` | string \| null | no | Match strategy. One of `"exact"`, `"contains"`, `"begins_with"`, `"ends_with"`. Defaults to a permissive match that may also return sub-domains. |

### Tool: `find_resource_acl_entry`

Look up the entry of a Compute ACL that covers a given IP. Backed by Fastly's dedicated `/resources/acls/{acl_id}/lookup?ip={ip}` endpoint — single API call, no enumeration of the (potentially millions of) entries the ACL holds. Returns the matching CIDR `prefix` and its `action` (typically `ALLOW` / `BLOCK`). On 404 (unknown ACL id *or* unmatched IP — Fastly does not distinguish), the tool returns a plain-text "no match" message.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `acl_id` | string | yes | Compute ACL identifier — typically obtained from `list_resource_acls`'s `id` field. |
| `ip` | string | yes | IPv4 or IPv6 address to look up against the ACL. |

### Tool: `get_service`

Fetch a service's metadata by `service_id`. Returns the service name, type (`vcl` / `wasm`), the currently-active `version` number, timestamps, and a `dependencies` map counting every config object attached to the active version (backends, directors, domains, healthchecks, plus the VCL-only object types when applicable).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier (e.g. `SU1Z0isxPaozGVKXdv0eY`). |

### Tool: `get_service_package`

Fetch the Compute (wasm) package metadata for a service version. Meaningful only for services of type `wasm`; on a VCL service or a version with no package uploaded, the tool returns a plain-text "no package" message instead of an error. Returns the package id, name, description, language, authors, size, `files_hash`, and timestamps.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect (typically the currently-active one, obtained via `get_service`). |

### Tool: `get_resource_config_store_item_value`

Fetch the value of a single key in a config store. Returns `{ config_store_id, key, item_value, created_at, updated_at }`. A 404 (unknown store or key — Fastly does not distinguish) is downgraded to plain text.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `config_store_id` | string | yes | Alphanumeric config-store identifier — typically obtained from `list_resource_config_stores`. |
| `key` | string | yes | Key to read. Listed by `list_resource_config_store_items`. |

### Tool: `get_resource_kv_store_item_value`

Fetch the value of a single key in a KV store. Returns `{ store_id, key, value }` with the value decoded as UTF-8 (binary blobs that don't round-trip through UTF-8 will lose fidelity). A 404 is downgraded to plain text. Internally bypasses the upstream SDK (which mis-handles raw response bodies) and issues a raw HTTPS GET reusing the shared `reqwest::Client`, auth header, and User-Agent.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `store_id` | string | yes | Alphanumeric KV-store identifier — typically obtained from `list_resource_kv_stores`. |
| `key` | string | yes | Key to read. Listed by `list_resource_kv_store_items`. |

### Tool: `list_resource_acls`

List the Fastly account's Compute ACLs (catalog only). Each entry returns `id` and `name`. Individual entries are intentionally not enumerated — a single Compute ACL can hold millions of prefixes, so use `find_resource_acl_entry` for IP-targeted lookups instead.

This tool takes no arguments — Fastly returns the full Compute ACL catalog in a single call.

### Tool: `list_resource_config_stores`

List the Fastly account's config stores enriched with their current `item_count`. Composes two upstream Fastly endpoints (`list_config_stores` + per-store `get_config_store_info`) into a single agent-facing call.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `name` | string \| null | no | Optional exact-name filter forwarded to Fastly. |

### Tool: `list_resource_config_store_items`

List the keys of a single config store. Returns keys only, not key/value pairs — read individual values with `get_resource_config_store_item_value`. A 404 (unknown store id) is downgraded to plain text.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `config_store_id` | string | yes | Alphanumeric config-store identifier. |

### Tool: `list_resource_kv_stores`

List the Fastly account's KV stores. Cursor-paginated. KV stores have no `item_count` because Fastly intentionally does not expose one (stores can hold millions of keys).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `name` | string \| null | no | Optional exact-name filter forwarded to Fastly. |
| `cursor` | string \| null | no | Pagination cursor — pass the `next_cursor` returned by a previous call. |
| `limit` | int32 \| null | no | Page size (Fastly default applies when unset). |

### Tool: `list_resource_kv_store_items`

List the keys of a single KV store. Returns keys only — Fastly KV deliberately offers no bulk-listing of values; read each value individually with `get_resource_kv_store_item_value`. Cursor-paginated, with an optional server-side `prefix` filter. A 404 (unknown store id) is downgraded to plain text.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `store_id` | string | yes | Alphanumeric KV-store identifier. |
| `prefix` | string \| null | no | Optional key-prefix filter forwarded to Fastly. |
| `cursor` | string \| null | no | Pagination cursor. |
| `limit` | int32 \| null | no | Page size. |

### Tool: `list_resource_secret_stores`

List the Fastly account's secret stores. Cursor-paginated. Returns `id`, `name`, `created_at` (Fastly intentionally keeps secret-store metadata minimal).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `name` | string \| null | no | Optional exact-name filter. |
| `cursor` | string \| null | no | Pagination cursor. |
| `limit` | int32 \| null | no | Page size (max 200). |

### Tool: `list_resource_secret_store_items`

List the secrets in a single secret store. Returns `name`, opaque `digest` (useful to detect rotations), and `created_at` — **never the value**. Secret values are reachable only at runtime from VCL or Compute, never via the Fastly management API; this MCP cannot bypass that contract, which is why there is intentionally no `get_resource_secret_store_item_value` tool. Cursor-paginated. A 404 is downgraded to plain text.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `store_id` | string | yes | Alphanumeric secret-store identifier. |
| `cursor` | string \| null | no | Pagination cursor. |
| `limit` | int32 \| null | no | Page size (max 200). |

### Tool: `list_service_versions`

List a service's currently-active version plus any open draft versions sitting above it. Locked historical versions and post-rollback artifacts are filtered out.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |

### Tool: `list_service_backends`

Return a JSON array of slim backend summaries for `(service_id, version)`. Each entry includes name, address, port, hostname/override-host, TLS posture (`use_ssl`, `ssl_check_cert`, min/max TLS version, cert/SNI hostnames), routing (`request_condition`, weight, `auto_loadbalance`, shielding), the bound healthcheck name, and the main timeouts (`connect_timeout`, `first_byte_timeout`, `between_bytes_timeout`, `max_conn`).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect (typically the currently-active one, obtained via `get_service`). |

### Tool: `list_service_dictionaries`

List the edge dictionaries declared in a service version. Each entry includes `id`, `name`, `write_only`, `item_count`, `digest`, and `last_updated`. Items themselves are no longer embedded — fetch them with `list_service_dictionary_items`, which lets the agent triage on item count first and only expand the dictionaries that matter.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_dictionary_items`

Fetch the key/value items of a single edge dictionary. Items are not version-scoped — Fastly manages dictionary contents out-of-band of versioned config — so this tool only takes `(service_id, dictionary_id)`. Supports Fastly pagination via `page` / `per_page`. A 404 (unknown service / dictionary) is downgraded to plain text; a 403 (`write_only: true` dictionary) returns a clean "write-only, items are not readable" message.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `dictionary_id` | string | yes | Dictionary identifier — typically obtained from `list_service_dictionaries`'s `id` field. |
| `page` | int32 \| null | no | 1-based page number. |
| `per_page` | int32 \| null | no | Page size (Fastly's default is small — 100 items). |

### Tool: `list_service_directors`

List the directors (load-balancing groups of backends) for a service version. Each director includes name, type (random/hash/client), quorum, retries, shielding POP, and the names of its member backends — cross-referenceable with `list_service_backends`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_domains`

List the FQDNs routed to a service version (legacy version-scoped domain view, complements `find_domain`'s account-wide DM v1 catalog).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_healthchecks`

List the healthcheck probes defined on a service version. Each entry contains the probe definition (name, host, path, method, http_version, expected_response, optional headers) and the decision parameters (check_interval, timeout, window, threshold, initial).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_resources`

List the account-scoped resources (KV / secret / config stores) linked to a specific service version. This is the bridge between the version-scoped service surface and the account-scoped store surface: each entry returns the link's `id`, `name`, the `resource_id` of the linked store, and the `resource_type` (`config` / `kv-store` / `secret-store`). The agent feeds `resource_id` to the matching `list_resource_*_items` tool to drill in. Compute ACLs are not surfaced here — query them via `list_resource_acls` instead.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_apex_redirects`

List the apex-domain redirects for a VCL service version. Each entry includes its HTTP `status_code` (301/302/307/308) and the array of `domains` the redirect applies to.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_cache_settings`

List the cache-settings rules for a VCL service version. Each rule includes name, action (`pass` / `cache` / `restart`), gating `cache_condition`, and TTL / stale-TTL.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_conditions`

List the named VCL boolean expressions for a VCL service version. Each condition includes name, type (`REQUEST` / `CACHE` / `RESPONSE` / `PREFETCH`), the VCL `statement`, and a string `priority`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_gzip`

List the gzip compression configurations for a VCL service version. Each entry includes the gating `cache_condition` and the space-separated `content_types` and `extensions` that should be compressed.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_headers`

List the header rules for a VCL service version. Each rule includes name, type (`request` / `cache` / `response`), action (`set` / `append` / `delete` / `regex` / `regex_repeat`), source/destination, optional regex/substitution, and any gating `*_condition` fields.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_rate_limiters`

List the rate limiters for a VCL service version. Each entry includes id, name, RPS limit, window size, client-key VCL variables, penalty-box duration, action (`response` / `response_object` / `log_only`), and (when applicable) the custom response or response-object name and the logger type.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_request_settings`

List the request-settings rules for a VCL service version. Each rule covers per-request flags such as `force_ssl`, `force_miss`, `default_host`, `hash_keys`, `xff` strategy, `geo_headers`, `timer_support`, and `max_stale_age`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_response_objects`

List the response objects (canned HTTP responses) for a VCL service version. Each entry includes name, status, response phrase, content-type, the body `content`, and any `request_condition` / `cache_condition`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

### Tool: `list_service_vcl_snippets`

List the VCL snippets for a VCL service version. Each snippet includes id, name, type (the VCL phase: `init` / `recv` / `hash` / `hit` / `miss` / `pass` / `fetch` / `error` / `deliver` / `log`), `dynamic` flag (`"0"` / `"1"`), priority, and the literal VCL `content`.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `service_id` | string | yes | Alphanumeric Fastly service identifier. |
| `version` | int32 | yes | Service version number to inspect. |

---

## Configuration

The server reads configuration from layered sources (later sources override earlier ones):

1. Built-in defaults (`127.0.0.1:8000`, `https://api.fastly.com`).
2. `/etc/<crate-name>/config.json` if present.
3. A `.env` file in the working directory (auto-loaded).
4. Environment variables prefixed `APP_` with `__` as nested-field separator.

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `APP_FASTLY__API_TOKEN` | **yes** | — | Fastly management-API token (sent on every upstream call as `Fastly-Key`). |
| `APP_SERVER__HOST` | no | `127.0.0.1` | Bind address. Set to `0.0.0.0` inside containers. |
| `APP_SERVER__PORT` | no | `8000` | Bind port. Must be ≥ 1024 when running as nonroot. |
| `APP_FASTLY__BASE_URL` | no | `https://api.fastly.com` | Override only for staging/test proxies; production should leave this alone. |

---

## Use this MCP Server

### Run the container

```bash
docker run --rm \
  -p 8000:8000 \
  -e APP_FASTLY__API_TOKEN=<your-fastly-token> \
  --name claude-mcp-fastly \
  aartintelligent/claude-mcp-fastly:latest
```

The MCP endpoint is then reachable at `http://127.0.0.1:8000/mcp`.

### Configure the client

For Claude Code, add an entry to your `.mcp.json` (project) or `~/.claude.json` (global):

```json
{
  "mcpServers": {
    "fastly": {
      "url": "http://127.0.0.1:8000/mcp",
      "type": "http"
    }
  }
}
```

For Claude Desktop or any other MCP-aware client supporting streamable-HTTP transport, point it at the same URL.

### Verify

After (re)starting the client, the 30 `mcp__fastly__*` tools should appear in the tool list. Try `find_domain` with a hostname you know is on Fastly — a successful response confirms the token is valid and the connection is healthy.

### Why run MCP servers in Docker?

Running an MCP server in a container limits its blast radius: the Fastly token is injected at runtime, the binary runs as `nonroot` on a hardened Debian base with no shell or package manager, and the server has no write access to the host filesystem. The image is built from Docker Hardened Images on every release, with the build pipeline pinned to a specific Rust toolchain (MSRV 1.95.0) and a deterministic Cargo.lock.
