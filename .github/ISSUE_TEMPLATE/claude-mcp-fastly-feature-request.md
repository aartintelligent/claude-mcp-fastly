---
name: Feature request
about: Propose a new tool, an additional argument on an existing tool, or a server-level capability.
title: "[feature] <short summary>"
labels: ["enhancement", "triage"]
assignees: []
---

<!--
Before opening this request, please skim CONTRIBUTING.md — specifically the
"Conventions that must not regress" section. Proposals that require
relaxing one of the conventions (adding write paths, returning secret-store
values, embedding raw upstream payloads instead of a slim projection,
breaking the `(service_id, version)` arg pattern, etc.) will almost always
be answered with "a new tool, not a weakened existing pattern". That is
still a valid discussion — just be aware of the bar.
-->

## Problem

<!--
Describe the concrete use-case first, not the solution. What audit /
inspection workflow are you trying to run, and which gap in the current
tool surface stops you?
"I want to know which CDN config currently routes traffic for foo.example.com"
is a problem.
"Add a `find_active_routing` tool" is a solution — save it for the next section.
-->

## Proposed change

<!--
If you already have an idea of the shape, sketch it: tool name, args, and
an example of the output JSON. If you don't, that's fine — say so and
leave the design to the discussion.
-->

```jsonc
// Tool: <new_tool_name>
// Args:
{
  "service_id": "<…>",
  "version": 12
}

// Output:
{
  // …
}
```

## Alternatives considered

<!--
What did you try first with the existing 30 tools? Why didn't it work?
Examples: chaining `get_service` + `list_service_*`, using the Fastly
dashboard, calling the Fastly API directly, scripting around the
existing surface. This section is the one that most often changes the
outcome of the discussion.
-->

## Impact on the public contract

<!-- Tick every box that applies. -->

- [ ] Adds a new tool — no existing tool changes
- [ ] Adds a new argument to an existing tool (additive, defaults to current behaviour)
- [ ] Changes the output shape of an existing tool (breaking)
- [ ] Changes the argument signature of an existing tool (breaking)
- [ ] Relaxes one of the conventions (read-only / slim projection / `(service_id, version)` / 404→text / secret-store contract — see CLAUDE.md)
- [ ] Adds a new dependency — please name it:
- [ ] None of the above / not sure yet

## Fastly endpoint coverage

<!--
If this proposal maps to a Fastly management-API endpoint that the server
does not currently call, list the endpoint and link to the Fastly doc.
Mention whether it is multi-kind, VCL-only, or Compute-only — that
determines which group it belongs to in INSTRUCTIONS / README.md.
-->

- **Endpoint:** `GET /…`
- **Doc:** https://www.fastly.com/documentation/reference/api/…
- **Group:** entry point / account-scoped / multi-kind / Compute-only / VCL-only

## MSRV implication

<!-- Would this require a newer Rust version than the current MSRV (1.95.0)? -->

- [ ] No
- [ ] Yes — minimum required version:
- [ ] Not sure

## Additional context

<!-- Prior art (similar tools in other MCP servers, Fastly Terraform provider, fastly CLI), screenshots of the API doc, links to related issues, etc. -->
