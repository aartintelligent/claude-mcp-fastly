# Contributing to claude-mcp-fastly

Thanks for taking the time to contribute. This server is small on purpose — its whole value comes from the narrow guarantees it preserves (read-only, slim projection, predictable shape) — so contributions are held to a correspondingly strict bar. Reading this page before opening a PR will save everyone time.

## Before you start

- **Open an issue first** for anything larger than a typo or a missing doc link. Some PRs are easier to reject than to review, especially anything that touches the tool surface or the upstream Fastly contract.
- **Security-sensitive reports** (token leakage paths, request smuggling against the MCP transport, unintended exposure of secret-store metadata) should go through a private channel rather than a public issue. Open a GitHub security advisory or email the maintainer listed in `Cargo.toml`.
- **Scope discipline.** A bug fix fixes the bug; please do not bundle refactors, renames, or "while I'm here" cleanups. They make regressions hard to bisect.

## Development setup

Requirements:

- Rust `1.95+` (edition 2024 — declared as the MSRV in `Cargo.toml`).
- `cargo` (comes with rustup).
- A Fastly management-API token to exercise the server end-to-end (set `APP_FASTLY__API_TOKEN` in `.env`).

Clone and bootstrap:

```bash
git clone https://github.com/aartintelligent/claude-mcp-fastly.git
cd claude-mcp-fastly
cp .env.dist .env   # then fill APP_FASTLY__API_TOKEN
cargo test
cargo run           # starts the server at 127.0.0.1:8000/mcp
```

## The local gates (run before every commit)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

A change is only ready for review once **all** of these succeed. If you discover a flaky test, file an issue rather than re-running until it passes.

## Project conventions (hard rules)

These are not style preferences — PRs that break them will be asked to revise:

- **`unsafe_code = "forbid"`** in `Cargo.toml`. No `unsafe`, anywhere.
- **`unused_must_use = "deny"`** and **`clippy::pedantic = "warn"`** are configured in `Cargo.toml`; lints get fixed, not suppressed without justification.
- **No `as` in `use` statements.** If two names collide, rename at the definition site or wrap in a module.
- **No `unwrap()` in production code.** Tests may use `expect("message")` with a descriptive message.
- **Explicitly typed public signatures.** No reliance on inference for public items.
- **English comments and identifiers.** The rustdoc and the user-facing tool descriptions are the project's surface; they stay in English.

## Conventions that must not regress

This server's value depends on a narrow contract. Any PR that touches the MCP layer (`src/mcp/`) or `src/app.rs` must preserve it:

- **Read-only.** No write paths, no provisioning, no caching of upstream payloads. Every tool invocation hits Fastly live.
- **Tool surface only.** The MCP capabilities advertised are tools; resources and tasks are intentionally not exposed. Adding either is a design change and needs an issue first.
- **Slim projection.** Every tool defines a private `XxxSummary<'a>` borrowing from the upstream `fastly_api` model. Drop redundant context fields (`service_id`, `version`, `deleted_at`), drop freeform `comment` fields, keep `created_at` / `updated_at` last. Use `#[serde(skip_serializing_if = "Option::is_none")]` so omitted fields disappear from the JSON.
- **`(service_id, version)` arg pattern** for every version-scoped `list_service_*` tool. The three version-agnostic entry points are `find_domain`, `get_service`, `list_service_versions` — do not add a fourth without strong justification.
- **404 → text, not error.** When Fastly returns 404 (unknown service / version / store / key / ACL), the tool returns `CallToolResult::success(Content::text("…not found…"))` rather than an `McpError`. Other Fastly errors propagate as `McpError::internal_error(format!("Fastly … failed: {e}"), None)` with a label that identifies the failing endpoint.
- **Secret-store values are never returned.** Fastly does not expose them at the management-API layer; this MCP must not work around that contract. There is intentionally no `get_resource_secret_store_item_value` tool.
- **Tool descriptions are short.** Keep `#[tool(description = "…")]` to a single sentence (~10 words). Cross-tool guidance lives in `INSTRUCTIONS` in `src/mcp/handler.rs`, not in per-tool descriptions.
- **VCL-only naming.** Tools that map to endpoints under Fastly's `/vcl-services/` doc tree carry the `list_service_vcl_*` prefix. Multi-kind tools use `list_service_*`.
- **Docker image tag is stable.** The canonical image reference is `aartintelligent/claude-mcp-fastly:latest`. Do not rename it without an explicit ask.

If you have a concrete use-case that seems to require relaxing one of these, open an issue first — the answer is usually "a new tool" rather than "weaken the existing pattern".

## Adding a new tool

The most common contribution. The shape:

1. Create `src/mcp/tools/<name>.rs` exporting a public `Args` struct (deriving `serde::Deserialize` + `schemars::JsonSchema`) and a public `async fn run(&AppState, Args) -> Result<CallToolResult, McpError>`.
2. Register it in `src/mcp/tools/mod.rs`.
3. Add a thin `#[tool(description = "…")]` adapter on `Handler` in `src/mcp/handler.rs` that delegates to `<name>::run`.
4. Update `INSTRUCTIONS` in `src/mcp/handler.rs` under the right section (entry points / account-scoped / multi-kind / Compute-only / VCL-only) and add cross-references where relevant.
5. Update `MCP.md` (the public catalog of tools) — add the tool to the table, the group list, the cross-references, and the per-tool details section.

If the tool wraps an SDK endpoint that is broken upstream (the `list_directors` workaround in `src/mcp/tools/list_service_directors.rs` is the canonical case), document the fallback at the call site so it can be removed in one shot when Fastly fixes the spec.

## Tests

- Unit tests live inline in `#[cfg(test)] mod tests` at the bottom of each module.
- New public behaviour needs a test. New 404-handling, projection, or cross-reference logic needs a test that would have failed before the change.
- Rustdoc examples in doc-comments are executed by `cargo test --doc` — keep them honest (no ` ```ignore ` to hide failures).

## Documentation

- The crate-level rustdoc and per-module rustdoc in `src/` are the canonical description of the code. Update them whenever the public surface changes.
- `MCP.md` is the shop window; update the tool table, the five-group list, the cross-references, and the per-tool details when you change behaviour.
- `CLAUDE.md` captures project-local context for AI-assisted editing; keep it in sync if you change conventions or tooling.
- `README.md` should stay aligned with `MCP.md`'s tool count and high-level shape.

## Changelog

Changelog entries are authored with [Changie](https://github.com/miniscruff/changie):

```bash
changie new
```

This drops a fragment under `.changes/unreleased/`. Commit the fragment with your change. Do **not** edit `CHANGELOG.md` directly — it is regenerated at release time via `changie batch <version>` + `changie merge`.

Pick the right kind (matches `.changie.yaml`):

| Kind         | Bumps    | Use for                                           |
| ------------ | -------- | ------------------------------------------------- |
| `Added`      | minor    | New tool, new entry point, new config knob        |
| `Changed`    | major    | Breaking change to a tool's args or output shape  |
| `Deprecated` | minor    | Tool or arg marked for removal but still works    |
| `Removed`    | major    | Tool deletion, arg deletion                       |
| `Fixed`      | patch    | Bug fixes that do not change the public contract  |
| `Security`   | patch    | Token-handling, transport, or leakage fixes       |

## Commit and pull-request etiquette

- **One logical change per PR.** Small PRs land faster and review more carefully.
- **Write commit messages in the imperative** (`Add …`, `Fix …`, `Refactor …`) and keep the subject under 70 characters. The body is the place for the *why*, not the *what*.
- **Rebase, don't merge**, when syncing with `master`. The history stays linear.
- **Mention the issue number** in the PR description when one exists.
- **CI must be green** before review (when CI is wired). If a flaky test bites you, file an issue rather than re-running until it passes.

## License

The project is licensed under the [MIT License](LICENSE). Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work shall be licensed as above, without any additional terms or conditions.
