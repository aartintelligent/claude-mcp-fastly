# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A read-only MCP (Model Context Protocol) server that surfaces Fastly account configuration to LLM agents. Each tool maps to a Fastly management-API call and returns a slim, agent-friendly projection of the upstream payload. No write paths, no caching — every invocation hits Fastly live.

## Common commands

- `cargo run` — start the server (binds `127.0.0.1:8000` by default; MCP endpoint at `/mcp`).
- `cargo build` / `cargo build --release` — release profile uses thin LTO, single codegen unit, stripped symbols.
- `cargo clippy --all-targets -- -D warnings` — lints are strict: `clippy::pedantic` is `warn`, `unsafe_code` is forbidden, `unused_must_use` is denied.
- `cargo fmt`.
- `cargo test` / `cargo test <name>` — there are no tests yet; add them next to the code they cover.
- `RUST_LOG=debug cargo run` — tracing is wired through `tracing-subscriber` with `EnvFilter`; defaults to `info`.
- `changie new` then `changie batch <version>` + `changie merge` — release notes flow through Changie (`.changes/unreleased/` → `CHANGELOG.md`). Do not hand-edit `CHANGELOG.md`.
- `docker build -t aartintelligent/claude-mcp-fastly:latest .` — canonical container image name & tag. Do not change the `aartintelligent/claude-mcp-fastly:latest` reference unless explicitly asked. The Dockerfile is a two-stage build on Docker Hardened Images (`dhi.io/rust:1.95-debian13-dev` for build, `dhi.io/debian-base:trixie-debian13` for runtime). Run with `docker run --rm -p 8000:8000 -e APP_FASTLY__API_TOKEN=<token> aartintelligent/claude-mcp-fastly:latest`.

Runtime configuration: env vars are prefixed `APP_` with `__` as the nested-field separator (e.g. `APP_SERVER__HOST=0.0.0.0 APP_FASTLY__API_TOKEN=… cargo run`). A `.env` is auto-loaded by `dotenvy`. `/etc/<crate-name>/config.json` is read if present (path derived from `CARGO_PKG_NAME` via `concat!(env!(...))`, so renaming the package retargets it automatically). The Fastly token (`APP_FASTLY__API_TOKEN`) is required — startup fails when absent.

## Architecture

`axum` + `rmcp` server exposing the tools surface over the streamable-HTTP transport (`/mcp`). Edition 2024, MSRV 1.95.0. Single binary, single Tokio runtime.

### Boot flow (`src/main.rs`)
`main` → `run` → `Config::load` → `AppState::new` → `build_router` → `serve`. Shutdown driven by a single `CancellationToken` cancelled on `SIGINT` (all platforms) or `SIGTERM` (Unix). The token is threaded into the rmcp transport so in-flight MCP sessions drain on `docker stop`.

### Configuration (`src/config.rs`)
Two structs:
- `ServerConfig { host, port }` — HTTP bind parameters, defaults `127.0.0.1:8000`.
- `FastlyConfig { base_url, api_token }` — `base_url` defaults to `https://api.fastly.com` via `set_default`; `api_token` is required (no default).

`PROJECT_NAME` const captures `env!("CARGO_PKG_NAME")` at compile time. Layered loading: built-in defaults → `/etc/<PROJECT_NAME>/config.json` (optional) → `.env` → `APP_*` env vars (last wins).

### Shared state (`src/app.rs`)
`AppState` is cloned on every handler invocation, so every field must be cheap to clone:
- `config: Arc<Config>` — resolved configuration.
- `fastly: Arc<FastlyConfiguration>` — pre-built `fastly_api` client config (`reqwest::Client` is internally `Arc`-shared).

`AppState::fastly_config()` returns an **owned** `FastlyConfiguration` cloned from the `Arc`. Owned because every `fastly_api` endpoint takes `&mut Configuration` (it mutates rate-limit counters from response headers). The clone is cheap — only the inner `Arc<reqwest::Client>` is bumped, no socket / pool duplication. Multiple parallel requests can therefore each take an independent `&mut` without contention.

`build_fastly_configuration` in `app.rs` is the only place that maps our `FastlyConfig` onto the SDK shape (`base_path`, `api_key`, defaults inherited via `..Default::default()`).

### MCP layer (`src/mcp/`)
Surface is **tools + prompts**. `resources` and `tasks` are intentionally not exposed.

- `server.rs::router` mounts `StreamableHttpService` at `/mcp` using a `LocalSessionManager`. Idle session keep-alive is bumped from rmcp's 5-min default to **30 minutes** (`SESSION_KEEP_ALIVE`) to keep MCP Inspector usable during interactive debugging. A fresh `Handler` is constructed per session.
- `handler.rs::Handler` is the single rmcp `ServerHandler` impl. Four macros coexist:
  - `#[tool_router]` on an inherent impl — generates `Self::tool_router()` and per-tool dispatch from `#[tool(...)]` methods.
  - `#[prompt_router]` on a *separate* inherent impl — generates `Self::prompt_router()` and per-prompt dispatch from `#[prompt(...)]` methods.
  - `#[tool_handler]` and `#[prompt_handler]` *stacked* on the same `impl ServerHandler` block — wires `tools/list` + `tools/call` and `prompts/list` + `prompts/get` respectively. Stacking requires explicit `#[tool_handler]` / `#[prompt_handler]` attributes (not the unified `server_handler` shortcut).
  
  Both router fields (`tool_router`, `prompt_router`) are read by macro-generated code only (`#[allow(dead_code)]`). `Handler::new` initializes both via `Self::tool_router()` + `Self::prompt_router()`. `get_info` advertises `enable_tools().enable_prompts()`; the protocol version is pinned to `V_2025_11_25`. `with_instructions(INSTRUCTIONS)` ships a terse server-level guidance string (defined as a `const` at the bottom of the file) that tells agents the workflow, the entry-point tools, and the multi-kind / VCL-only split. Long-form guidance lives in the `agent_system` prompt instead.
- `tools/` is the routing surface for tools. Each module exports:
  - a public `Args` struct deriving `serde::Deserialize` + `schemars::JsonSchema`,
  - a public `async fn run(&AppState, Args) -> Result<CallToolResult, McpError>`.
  
  The `#[tool(...)]` method on `Handler` stays a thin adapter that just delegates to `run`.
- `prompts/` is the routing surface for prompts. Each module exports a public `run` function returning `Result<Vec<PromptMessage>, McpError>` (the `Result` is required by rmcp's prompt-handler signature even when infallible). The `#[prompt(...)]` method on `Handler` is a thin adapter to `run`. The current single prompt is `agent_system`, which embeds its content from `prompts/agent_system.md` via `include_str!` so the canonical playbook is authored as Markdown, not as a Rust string literal. That same `.md` is the source of truth for the Claude Code subagent at `.claude/agents/fastly_specialist.md`, which is a thin wrapper that delegates to it.

### Errors (`src/error.rs`)
All fallible code returns `crate::error::Result<T>`. `AppError` keeps three broad variants (`Io`, `Config`, `Other(anyhow::Error)`) with `#[error(transparent)]` so the originating error's `Display`/`source` chain is preserved. Use `?` with `anyhow::Error` for ad-hoc errors; promote a variant only when callers need to match on it.

## Tool conventions (this codebase, on top of the rmcp shape)

- **Slim projection.** Every tool defines a private `XxxSummary<'a>` struct that borrows from the upstream `fastly_api` model. Drop redundant context fields (`service_id`, `version`, `deleted_at`), drop freeform `comment` fields, keep `created_at`/`updated_at` at the end. Use `#[serde(skip_serializing_if = "Option::is_none")]` so omitted fields disappear from the JSON.
- **`(service_id, version)` arg pattern.** Every version-scoped `list_service_*` tool takes `service_id: String` + `version: i32`. The agent is expected to call `get_service` first to learn the active version. Three tools are version-agnostic entry points: `find_domain`, `get_service`, `list_service_versions`.
- **404 → text, not error.** When Fastly returns 404 (unknown service / version), the tool returns a `CallToolResult::success(Content::text("…not found…"))` instead of an `McpError`. The agent gets a clean, actionable signal it can summarize. Other Fastly errors propagate as `McpError::internal_error(format!("Fastly … failed: {e}"), None)` with a label that identifies the failing endpoint.
- **Tool descriptions.** Keep the `#[tool(description = "…")]` to a single short sentence (~10 words). The schema for the args is generated by `schemars`, so the agent sees param types and per-field docs there. Cross-tool guidance (workflows, chaining hints, the multi-kind / VCL-only split) lives in `INSTRUCTIONS` in `handler.rs`, not in per-tool descriptions.
- **VCL-only naming.** Tools that map to endpoints under Fastly's `/vcl-services/` doc tree carry the `list_service_vcl_*` prefix (the corresponding objects don't exist on Compute services). Multi-kind tools use `list_service_*`.

## SDK quirks worth knowing

- **Fastly returns several boolean-like flags as numeric strings** (`"0"` / `"1"`) on the request-settings, headers, and snippet endpoints. We forward them as `&str` and let the agent interpret.
- **`type` enums are sometimes string-tagged in the SDK but integer on the wire** (the director's `type` is the worst offender). Use `#[serde(untagged)]` enums or string passthrough when in doubt.
- **`list_directors` is broken in the upstream SDK.** The OpenAPI spec declares `DirectorResponse.backends: Vec<Backend>`, but the live API returns `Vec<String>` (backend names). Any director with at least one backend fails to deserialize.
  
  Workaround lives in `src/mcp/tools/list_service_directors.rs`: a `pub(super)` helper `fetch_directors_raw` issues a raw HTTP `GET` against the same endpoint, reusing `state.fastly_config()`'s `reqwest::Client`, auth header, and User-Agent — only the typed deserialization is replaced (a local `DirectorRaw` struct with the correct shape). `src/mcp/tools/get_service.rs` reuses `fetch_directors_raw` for the directors count in its `dependencies` map. If Fastly fixes their spec, the workaround can be deleted in one shot — search for `pub(super) async fn fetch_directors_raw` and remove that section + the SDK call's `use` statements.
- **`list_dictionaries` items aren't in the same call.** `list_service_dictionaries` composes two endpoints (`list_dictionaries` + per-dict `list_dictionary_items`) into a single agent-facing response. Items are nested inside their parent dictionary; write-only dictionaries omit `items` because Fastly forbids reading their values.

## Conventions

- **Adding a new tool:** create `src/mcp/tools/<name>.rs` with `Args` + `run`, register it in `src/mcp/tools/mod.rs`, then add a `#[tool(description = "…")]` method on `Handler` that delegates to `<name>::run`. If the tool covers a class of objects the agent will likely chain to, also update `INSTRUCTIONS` in `handler.rs` (under the right section: entry points / multi-kind / VCL-only) and add cross-references where relevant. The long-form playbook lives in `src/mcp/prompts/agent_system.md` — update the relevant tool inventory section there too.
- **Adding a new prompt:** create `src/mcp/prompts/<name>.rs` with a `run` function returning `Result<Vec<PromptMessage>, McpError>`, register it in `src/mcp/prompts/mod.rs`, then add a `#[prompt(name = "<name>", description = "…")]` method on `Handler` (inside the existing `#[prompt_router] impl Handler` block) that delegates to `<name>::run`. If the prompt content is more than a few lines, author it as a sibling `.md` file and include it via `include_str!`.
- **Extending `Config`:** add a field to the relevant struct in `src/config.rs` and a default via `set_default(...)` in `Config::load` if there is a sensible one. Document the env-var form (`APP_<SECTION>__<FIELD>`) in `.env.dist`.
- **Calling Fastly:** prefer the `fastly_api` SDK. Always do `let mut cfg = state.fastly_config();` per call (cheap). Only fall back to raw `reqwest` when the SDK is broken for a specific endpoint (currently only directors); document the fallback at the call site.
- **Parallel Fastly calls:** `tokio::try_join!` for fixed sets, with one cloned `FastlyConfiguration` per future. The macro `count_dep!` in `get_service.rs` is the canonical pattern when chaining many `list_*` calls of the same shape.
