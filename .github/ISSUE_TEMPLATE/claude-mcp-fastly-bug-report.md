---
name: Bug report
about: Report a defect in hosanna-rs-secret — incorrect behaviour, broken invariant, or compilation failure.
title: "[bug] <short summary>"
labels: ["bug", "triage"]
assignees: []
---

<!--
⚠ If your report concerns a security-sensitive issue — a leak through an
unexpected path, a timing side-channel in the constant-time comparison,
a failure of zeroisation on drop, or any way to obtain the raw secret
other than `ExposeSecret::expose_secret()` — do NOT file a public issue.
Open a private GitHub security advisory instead. See CONTRIBUTING.md.
-->

## Summary

<!-- One or two sentences: what goes wrong, and why you think it's a bug. -->

## Environment

- **Crate version:** `hosanna-rs-secret = "x.y.z"`
- **Rust version:** `rustc --version` output here
- **Target triple:** `rustc -vV | grep host` output here (e.g. `x86_64-unknown-linux-gnu`)
- **Feature flags in use:** e.g. `default`, or `default-features = false, features = ["serde"]`
- **OS:** Linux / macOS / Windows / …

## Minimal reproducer

<!--
Please reduce the problem to the smallest possible `main.rs` or `#[test]`.
The crate is tiny — most real bugs can be expressed in <30 lines.
-->

```rust
use hosanna_rs_secret::{ExposeSecret, SecretString};

fn main() {
    // …
}
```

## Expected behaviour

<!-- What you expected to happen, and why (pointer to rustdoc, README contract table, etc.). -->

## Actual behaviour

<!-- What actually happens: error message, wrong output, panic, miscompile, etc. Paste verbatim. -->

```text
<stdout / stderr / compiler output here>
```

## Does this touch a load-bearing invariant?

<!--
Tick every box that applies. These are the guarantees listed in the
public-contract table of the README. A "yes" on any of them makes the
issue higher priority.
-->

- [ ] `Display` / `Debug` leak the value
- [ ] `Serialize` was somehow implemented / exposed
- [ ] `Clone` was somehow implemented / exposed
- [ ] Memory is not zeroised on drop
- [ ] `PartialEq` is no longer constant-time
- [ ] Access to the raw value is possible without calling `expose_secret()`
- [ ] None of the above — this is a functional / ergonomic bug

## Additional context

<!-- Logs, screenshots, links to related issues, prior art, or anything else useful. -->
