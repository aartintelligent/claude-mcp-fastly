# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common commands

- `cargo run` — start the server (binds `127.0.0.1:8000` by default; MCP endpoint at `/mcp`).
- `cargo build` / `cargo build --release` — release profile uses thin LTO, single codegen unit, stripped symbols.
- `cargo clippy --all-targets -- -D warnings` — lints are strict: `clippy::pedantic` is `warn`, `unsafe_code` is forbidden, `unused_must_use` is denied. Treat clippy output as part of the build.
- `cargo fmt`.
- `cargo test` / `cargo test <name>` — there are no tests yet; add them next to the code they cover.
- `RUST_LOG=debug cargo run` — tracing is wired through `tracing-subscriber` with `EnvFilter`; defaults to `info`.
- `changie new` then `changie batch <version>` + `changie merge` — release notes flow through Changie (`.changes/unreleased/` → `CHANGELOG.md`). Do not hand-edit `CHANGELOG.md`.

Configuration override at runtime: env vars are prefixed `APP_` with `__` as the nested-field separator, e.g. `APP_SERVER__HOST=0.0.0.0 APP_SERVER__PORT=9000 cargo run`. A `.env` is auto-loaded by `dotenvy`. `/etc/mcp/config.json` is read if present.

## Architecture

This is an MCP (Model Context Protocol) server built on `axum` + `rmcp`, exposing tools and long-running tasks over the streamable-HTTP transport (`/mcp`). The toolchain targets Rust edition 2024, MSRV 1.95.0.

### Boot flow (`src/main.rs`)
`main` → `run` → `Config::load` → `AppState::new` → `build_router` → `serve`. Shutdown is driven by a single `CancellationToken` cancelled on `SIGINT` (all platforms) or `SIGTERM` (Unix). The token is threaded into the rmcp transport so in-flight MCP sessions drain on `docker stop`.

### Router composition (`src/main.rs::build_router`)
The top-level `Router` is built by **merging** feature-area sub-routers, each typed as `Router<AppState>`, and attaching state once via `with_state`. New REST endpoints (or additional MCP-adjacent services) should follow the same shape: build a `Router<AppState>` in their own module, expose a `pub fn router(...)`, and merge it here. Don't attach state inside sub-routers.

### Shared state (`src/app.rs`)
`AppState` is cloned on every handler invocation, so every field must be cheap to clone. Today the only field is `Arc<Config>`. New services go in as `Arc`-wrapped fields — never owning values — to keep `Clone` constant-time.

### MCP layer (`src/mcp/`)
- `server.rs::router` mounts `StreamableHttpService` at `/mcp` using a `LocalSessionManager` (in-process session tracking). Idle session keep-alive is bumped from rmcp's 5-min default to **30 minutes** (`SESSION_KEEP_ALIVE`), to keep MCP Inspector usable during interactive debugging. A fresh `Handler` is constructed per session.
- `handler.rs::Handler` is the single rmcp `ServerHandler` impl. It uses three macros that must coexist:
  - `#[tool_router]` on the inherent impl — generates `Self::tool_router()` and per-tool dispatch from `#[tool(...)]` methods.
  - `#[tool_handler]` on `impl ServerHandler` — wires `tools/list` + `tools/call`.
  - `#[task_handler]` on the same `impl ServerHandler` — wires `tasks/get`, `tasks/result`, `tasks/list`, `tasks/cancel`, dispatched through the `processor: Arc<Mutex<OperationProcessor>>` field.
  Both `tool_router` and `processor` fields are read by macro-generated code only (hence `#[allow(dead_code)]`). `get_info` advertises capabilities; the protocol version is pinned to `V_2025_11_25`.
- `tools/` and `tasks/` are **conventions**, not separate routing surfaces. Both kinds of modules expose the same shape:
  - a public `Args` struct deriving `serde::Deserialize` + `schemars::JsonSchema` (used for both decoding and the `tools/list` JSON schema),
  - a public `async fn run(&AppState, Args) -> Result<CallToolResult, McpError>`.
  The `#[tool(...)]` method on `Handler` stays a thin adapter that just delegates to `run`. The split is purely about intent: tools in `tasks/` are long-running and expected to be invoked with a `task` field. `slow_demo` further enforces this with `execution(task_support = "required")` — a plain synchronous call is rejected with `-32601`.
- `prompts/` and `resources/` are placeholder modules. To enable them, add a `#[prompt_router]` / `#[resource_router]` block in `handler.rs` and flip the matching capability flag in `get_info`.

### Errors (`src/error.rs`)
All fallible code returns `crate::error::Result<T>`. `AppError` keeps three broad variants (`Io`, `Config`, `Other(anyhow::Error)`) with `#[error(transparent)]` so the originating error's `Display`/`source` chain is preserved verbatim. Use `?` with `anyhow::Error` for ad-hoc errors; promote a variant only when callers need to match on it.

## Conventions

- Adding a new tool: create `src/mcp/tools/<name>.rs` with `Args` + `run`, register it in `src/mcp/tools/mod.rs`, then add a `#[tool(description = "...")]` method on `Handler` that delegates to `<name>::run`. The tool description is the only place the human-readable name lives — keep it accurate.
- Adding a long-running tool: same shape under `src/mcp/tasks/`, and consider `execution(task_support = "required")` on the handler method if a synchronous call would not make sense.
- When extending `Config`, add a field to the relevant struct in `src/config.rs` and a default via `set_default(...)` in `Config::load` if there is a sensible one. Document the env-var form (`APP_<SECTION>__<FIELD>`) in `.env.dist`.
