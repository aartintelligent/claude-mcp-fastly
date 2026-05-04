# Changelog

All notable changes to this project will be documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases are managed by [release-please](https://github.com/googleapis/release-please) — do not hand-edit this file.

## [0.1.0](https://github.com/aartintelligent/claude-mcp-fastly/compare/v0.0.2...v0.1.0) (2026-05-04)


### ⚠ BREAKING CHANGES

* contributor workflow changes. Authors no longer run `changie new` — release-please derives the changelog from commit messages directly. PRs labelled `skip-changelog` are no longer needed (the label can be deleted from the repo).

### CI

* migrate from Changie to release-please ([#4](https://github.com/aartintelligent/claude-mcp-fastly/issues/4)) ([ef3b44a](https://github.com/aartintelligent/claude-mcp-fastly/commit/ef3b44a2792e522323d4ee4410573380b7c0e4ba))

## 0.0.2 (2026-05-04)

### Fixed

* Correct typo in `Cargo.toml` `keywords` (was `faslty`, now `fastly`) so the crate is discoverable by its primary domain keyword on crates.io.
