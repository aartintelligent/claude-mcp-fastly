---
name: Bug report
about: Report a defect in claude-mcp-fastly — a tool returning the wrong shape, a 404 surfacing as an error, a panic, or a startup failure.
title: "[bug] <short summary>"
labels: ["bug", "triage"]
assignees: []
---

<!--
⚠ If your report concerns a security-sensitive issue — a path through which
a Fastly API token could leak, a way to obtain a secret-store value via
the MCP surface, an MCP transport issue with auth implications, or any
unintended exposure of credentials — do NOT file a public issue.
Open a private GitHub security advisory instead. See CONTRIBUTING.md.
-->

## Summary

<!-- One or two sentences: which tool (or boot path) misbehaves, and why you think it's a bug. -->

## Environment

- **Server version:** image tag or commit SHA (e.g. `aartintelligent/claude-mcp-fastly:latest @ <sha>`)
- **Deployment:** Docker / `cargo run` / other
- **Rust version (only if running from source):** `rustc --version` output
- **OS:** Linux / macOS / Windows / …
- **MCP client:** Claude Code / Claude Desktop / Cursor / MCP Inspector / other
- **Fastly account scope:** Compute-only / VCL-only / mixed (if relevant)

## Reproducer

<!--
A minimal sequence of MCP tool calls that triggers the bug. Redact any
real `service_id` / `store_id` / API token. The smaller the better — most
bugs can be reduced to one or two calls.
-->

```jsonc
// Tool: <tool_name>
// Args:
{
  "service_id": "<redacted>",
  "version": 12
}
```

## Expected behaviour

<!--
What you expected the tool (or the server) to return, and why.
Pointer to README.md tool details, CLAUDE.md invariants, or the upstream
Fastly API doc if relevant.
-->

## Actual behaviour

<!-- What actually happens: wrong JSON shape, MCP error, panic, 5xx, etc. Paste the verbatim response. -->

```text
<MCP response / server log / panic backtrace here>
```

## Does this touch a load-bearing invariant?

<!--
Tick every box that applies. These are the conventions listed in
CLAUDE.md / CONTRIBUTING.md. A "yes" on any of them makes the issue
higher priority.
-->

- [ ] Read-only contract violated (a tool wrote / mutated something)
- [ ] Tool output shape changed in a backwards-incompatible way (slim-projection regression)
- [ ] A 404 from Fastly surfaced as an `McpError` instead of plain-text "not found"
- [ ] A secret-store value was returned by the MCP (must never happen)
- [ ] A version-scoped `list_service_*` tool accepts a different arg pattern than `(service_id, version)`
- [ ] Server panics, hangs, or fails to drain in-flight sessions on shutdown
- [ ] Fastly token / `Fastly-Key` header leaks into a log, error message, or response
- [ ] None of the above — functional or ergonomic bug

## Additional context

<!-- Server logs (`RUST_LOG=debug`), screenshots, links to related issues or upstream Fastly tickets, anything else useful. Redact tokens. -->
