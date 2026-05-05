# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project purpose

`claude-mcp-fastly` is a Rust binary crate that runs a read-only MCP (Model Context Protocol) server exposing a Fastly account's configuration to LLM agents. The public surface is a set of tools, each mapping to a Fastly management-API call and returning a slim, agent-friendly projection of the upstream payload. No write paths, no caching — every invocation hits Fastly live.

`README.md` is the canonical description of the public tool surface (catalog, groups, cross-references, per-tool args). The per-module rustdoc in `src/` is the canonical description of the code. Read `README.md` first to understand what the server offers, then dive into `src/mcp/handler.rs` and `src/mcp/tools/` to see how the surface is wired.

## Commands

```bash
cargo check
cargo run                                            # binds 127.0.0.1:8000, MCP at /mcp
cargo build --release                                # thin LTO, single codegen unit, stripped
cargo test                                           # there are no tests yet — add them next to the code they cover
cargo test <test_name>                               # run a single test
cargo clippy --all-targets --locked -- -D warnings   # pedantic + missing_*_doc lints are active
cargo fmt --all -- --check                           # what the local gates run
cargo doc --open                                     # render the per-module rustdoc
RUST_LOG=debug cargo run                             # tracing via tracing-subscriber's EnvFilter (default: info)
```

Container image (canonical tag — do not rename without an explicit ask):

```bash
docker build -t aartintelligent/claude-mcp-fastly:latest .
docker run --rm -p 8000:8000 -e APP_FASTLY__API_TOKEN=<token> aartintelligent/claude-mcp-fastly:latest
```

Two-stage build on Docker Hardened Images: `dhi.io/rust:1.95-debian13-dev` for build, `dhi.io/debian-base:trixie-debian13` for runtime.

Releases are driven by [release-please](https://github.com/googleapis/release-please). Every push to `master` runs `release-please-action`, which maintains a single long-lived **Release PR** (`chore(release): X.Y.Z`) aggregating Conventional Commits since the last release. Merging that PR triggers the `publish` job — Docker push to `aartintelligent/claude-mcp-fastly:{version,latest}` and a GitHub Release. Do not hand-edit `CHANGELOG.md`; release-please owns it. Configuration lives in `release-please-config.json` and `.release.json`.

## Commit messages

Commits follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/). The commit message **is** the changelog source — release-please groups commits by type, computes the next semver, and writes the section. Be deliberate.

Format:

```
<type>[optional scope][!]: <short summary>

[optional body explaining the *why*]

[optional footer(s), e.g. BREAKING CHANGE:, Refs: #123]
```

Mapping of types to changelog sections and SemVer bumps (declared in `release-please-config.json`):

| CC type    | Changelog section | SemVer bump | Visible in changelog? |
| ---------- | ----------------- | ----------- | --------------------- |
| `feat`     | `Added`           | minor       | ✅ yes                |
| `fix`      | `Fixed`           | patch       | ✅ yes                |
| `perf`     | `Performance`     | patch       | ✅ yes                |
| `revert`   | `Reverted`        | patch       | ✅ yes                |
| `refactor` | (hidden)          | —           | ❌ no                 |
| `docs`     | (hidden)          | —           | ❌ no                 |
| `test`     | (hidden)          | —           | ❌ no                 |
| `chore`    | (hidden)          | —           | ❌ no                 |
| `ci`       | (hidden)          | —           | ❌ no                 |
| `build`    | (hidden)          | —           | ❌ no                 |
| `style`    | (hidden)          | —           | ❌ no                 |

Breaking changes — indicated either by the `!` suffix (`feat!:`, `fix!:`) or by a `BREAKING CHANGE:` footer — bump major. Until the project ships `1.0.0`, `bump-minor-pre-major` keeps breaking changes at minor and `bump-patch-for-minor-pre-major` keeps `feat` at patch — this matches SemVer's pre-1.0 convention.

For security-sensitive fixes (token-handling, transport, leakage paths), use `fix(security):` so the scope makes the intent searchable in git history; release-please will still file it under `Fixed`.

## Pre-commit / local gates

There is no `.cargo-husky` hook wired in this repository (yet). The same gates the future hook will run are listed below — execute them before every commit:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Bypass any future hook only in emergencies with `git commit --no-verify`. If/when a hook is added, its source lives in `.cargo-husky/hooks/pre-commit` (the versioned copy) — edits to `.git/hooks/pre-commit` are local and get overwritten.

## Architecture

`axum` + `rmcp` server exposing the tools surface over the streamable-HTTP transport (`/mcp`). Edition 2024, MSRV 1.95.0. Single binary, single Tokio runtime.

### Boot flow (`src/main.rs`)

`main` → `telemetry::init` → `Config::load` → `AppState::new` → `build_router` → `serve`. Shutdown via `shutdown::wait` is driven by a single `CancellationToken` cancelled on `SIGINT` (all platforms) or `SIGTERM` (Unix). The token is threaded into the rmcp transport so in-flight MCP sessions drain on `docker stop`. Tracing setup lives in `src/telemetry.rs`, signal handling in `src/shutdown.rs`.

### Configuration (`src/config.rs`)

Two structs:

- `ServerConfig { host, port }` — HTTP bind parameters, defaults `127.0.0.1:8000`.
- `FastlyConfig { base_url, api_token }` — `base_url` defaults to `https://api.fastly.com` via `set_default`; `api_token` is required (no default).

`PROJECT_NAME` const captures `env!("CARGO_PKG_NAME")` at compile time. Layered loading: built-in defaults → `/etc/<PROJECT_NAME>/config.json` (optional) → `.env` → `APP_*` env vars (last wins). Env vars are prefixed `APP_` with `__` as the nested-field separator (e.g. `APP_SERVER__HOST=0.0.0.0 APP_FASTLY__API_TOKEN=… cargo run`). Document any new env-var form in `.env.dist`.

### Shared state (`src/app.rs`)

`AppState` is cloned on every handler invocation, so every field must be cheap to clone:

- `config: Arc<Config>` — resolved configuration.
- `fastly: Arc<FastlyConfiguration>` — pre-built `fastly_api` client config (`reqwest::Client` is internally `Arc`-shared).

`AppState::fastly_config()` returns an **owned** `FastlyConfiguration` cloned from the `Arc`. Owned because every `fastly_api` endpoint takes `&mut Configuration` (it mutates rate-limit counters from response headers). The clone is cheap — only the inner `Arc<reqwest::Client>` is bumped, no socket / pool duplication. Multiple parallel requests can therefore each take an independent `&mut` without contention.

`build_fastly_configuration` in `app.rs` is the only place that maps our `FastlyConfig` onto the SDK shape (`base_path`, `api_key`, defaults inherited via `..Default::default()`).

### MCP layer (`src/mcp/`)

The advertised surface is **tools only**. Resources, prompts, and tasks are intentionally not exposed.

- `server.rs::router` mounts `StreamableHttpService` at `/mcp` using a `LocalSessionManager`. Idle session keep-alive is bumped from rmcp's 5-min default to **30 minutes** (`SESSION_KEEP_ALIVE`) to keep MCP Inspector usable during interactive debugging. A fresh `Handler` is constructed per session.
- `handler.rs::Handler` is the single rmcp `ServerHandler` impl. Two macros coexist:
  - `#[tool_router]` on an inherent impl — generates `Self::tool_router()` and per-tool dispatch from `#[tool(...)]` methods.
  - `#[tool_handler]` on the `impl ServerHandler` block — wires `tools/list` + `tools/call`.

  The `tool_router` field is read by macro-generated code only (`#[allow(dead_code)]`). `Handler::new` initializes it via `Self::tool_router()`. `get_info` advertises `enable_tools()`; the protocol version is pinned to `V_2025_11_25`. `with_instructions(INSTRUCTIONS)` ships a terse server-level guidance string (defined as a `const` at the bottom of the file) that tells agents the workflow, the entry-point tools, and the multi-kind / Compute-only / VCL-only split.
- `tools/` is the routing surface. Each module exports:
  - a public `Args` struct deriving `serde::Deserialize` + `schemars::JsonSchema`,
  - a public `async fn run(&AppState, Args) -> Result<CallToolResult, McpError>`.

  The `#[tool(...)]` method on `Handler` stays a thin adapter that just delegates to `run`.

### Errors (`src/error.rs`)

All fallible code returns `crate::error::Result<T>`. `AppError` keeps three broad variants (`Io`, `Config`, `Other(anyhow::Error)`) with `#[error(transparent)]` so the originating error's `Display`/`source` chain is preserved. Use `?` with `anyhow::Error` for ad-hoc errors; promote a variant only when callers need to match on it.

### Invariants the tools must preserve

These are load-bearing — do not weaken them without a deliberate, reviewed change:

- **Read-only.** No write paths, no provisioning, no caching of upstream payloads. Every tool invocation hits Fastly live.
- **Slim projection.** Every tool defines a private `XxxSummary<'a>` borrowing from the upstream `fastly_api` model. Drop redundant context fields (`service_id`, `version`, `deleted_at`), drop freeform `comment` fields, keep `created_at` / `updated_at` last. Use `#[serde(skip_serializing_if = "Option::is_none")]` so omitted fields disappear from the JSON.
- **`(service_id, version)` arg pattern.** Every version-scoped `list_service_*` tool takes `service_id: String` + `version: i32`. The agent is expected to call `get_service` first to learn the active version. The version-agnostic entry points are `find_domain`, `get_service`, `list_service_versions`.
- **404 → text, not error.** When Fastly returns 404 (unknown service / version / store / key / ACL), the tool returns `CallToolResult::success(Content::text("…not found…"))` rather than an `McpError`. Other Fastly errors propagate as `McpError::internal_error(format!("Fastly … failed: {e}"), None)` with a label that identifies the failing endpoint.
- **Secret-store values are never returned.** Fastly does not expose them at the management API; this MCP must not work around that contract. There is intentionally no `get_resource_secret_store_item_value` tool.
- **Tool descriptions are short.** Keep `#[tool(description = "…")]` to a single sentence (~10 words). The schema for the args is generated by `schemars`, so the agent sees param types and per-field docs there. Cross-tool guidance (workflows, chaining hints, the multi-kind / Compute-only / VCL-only split) lives in `INSTRUCTIONS` in `handler.rs`, not in per-tool descriptions.
- **VCL-only naming.** Tools that map to endpoints under Fastly's `/vcl-services/` doc tree carry the `list_service_vcl_*` prefix. Multi-kind tools use `list_service_*`.
- **Docker image tag is stable.** The canonical reference is `aartintelligent/claude-mcp-fastly:latest`. Do not rename it without an explicit ask.

## SDK quirks worth knowing

- **Fastly returns several boolean-like flags as numeric strings** (`"0"` / `"1"`) on the request-settings, headers, and snippet endpoints. We forward them as `&str` and let the agent interpret.
- **`type` enums are sometimes string-tagged in the SDK but integer on the wire** (the director's `type` is the worst offender). Use `#[serde(untagged)]` enums or string passthrough when in doubt.
- **`list_directors` is broken in the upstream SDK.** The OpenAPI spec declares `DirectorResponse.backends: Vec<Backend>`, but the live API returns `Vec<String>` (backend names). Any director with at least one backend fails to deserialize.

  Workaround lives in `src/mcp/tools/list_service_directors.rs`: a `pub(super)` helper `fetch_directors_raw` issues a raw HTTP `GET` against the same endpoint, reusing `state.fastly_config()`'s `reqwest::Client`, auth header, and User-Agent — only the typed deserialization is replaced (a local `DirectorRaw` struct with the correct shape). `src/mcp/tools/get_service.rs` reuses `fetch_directors_raw` for the directors count in its `dependencies` map. If Fastly fixes their spec, the workaround can be deleted in one shot — search for `pub(super) async fn fetch_directors_raw` and remove that section + the SDK call's `use` statements.
- **KV `get` returns a raw response body**, not JSON. The upstream SDK pipes it through `serde_json::from_str` and so only succeeds when the value happens to be a JSON-encoded string. Workaround in `src/mcp/tools/get_resource_kv_store_item_value.rs`: bypass the SDK and issue a raw HTTPS `GET`, decoding the bytes as UTF-8.
- **`list_service_dictionaries` items are not in the same call.** The catalog endpoint returns metadata only; items live under a separate, non-version-scoped endpoint (`list_service_dictionary_items`). Write-only dictionaries return 403 on the items endpoint — we downgrade that to a clean text signal.

## Adding a new tool

The most common contribution. The shape:

1. Create `src/mcp/tools/<name>.rs` with a public `Args` struct (`serde::Deserialize` + `schemars::JsonSchema`) and a public `async fn run(&AppState, Args) -> Result<CallToolResult, McpError>`.
2. Register it in `src/mcp/tools/mod.rs`.
3. Add a thin `#[tool(description = "…")]` adapter on `Handler` in `src/mcp/handler.rs` that delegates to `<name>::run`.
4. Update `INSTRUCTIONS` in `src/mcp/handler.rs` under the right group (entry points / account-scoped / multi-kind / Compute-only / VCL-only) and add cross-references where relevant.
5. Update `README.md` — add the tool to the table, the group list, the cross-references, and the per-tool details section.

If the tool wraps an SDK endpoint that is broken upstream (the `list_directors` and KV-`get` workarounds are the canonical cases), document the fallback at the call site so it can be removed in one shot when Fastly fixes the spec.

When extending `Config`: add a field to the relevant struct in `src/config.rs` and a default via `set_default(...)` in `Config::load` if there is a sensible one. Document the env-var form (`APP_<SECTION>__<FIELD>`) in `.env.dist`.

When calling Fastly: prefer the `fastly_api` SDK. Always do `let mut cfg = state.fastly_config();` per call (cheap). Only fall back to raw `reqwest` when the SDK is broken for a specific endpoint; document the fallback at the call site. For parallel Fastly calls, use `tokio::try_join!` for fixed sets, with one cloned `FastlyConfiguration` per future. The macro `count_dep!` in `get_service.rs` is the canonical pattern when chaining many `list_*` calls of the same shape.

## Project conventions (strict)

These are project-wide rules, not style preferences:

- **No `unsafe`** — enforced by `unsafe_code = "forbid"` in `Cargo.toml`.
- **No `as` in `use` statements** — never `use foo::Bar as Baz;`.
- **No `unwrap()`** in production code (tests may use `expect("…")` with a message).
- **`unused_must_use = "deny"`** and **`clippy::pedantic = "warn"`** in `Cargo.toml`; lints get fixed, not suppressed without justification. `missing_errors_doc` and `missing_panics_doc` are warned on, `missing_safety_doc` and `undocumented_unsafe_blocks` are denied.
- Public types are always explicitly typed (no inferred public signatures).
- English comments and identifiers — the rustdoc and the user-facing tool descriptions are the project's surface; they stay in English.
- `edition = "2024"`, MSRV `1.95.0` (declared in `Cargo.toml`).
