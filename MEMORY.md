# Project Memory

## Current State
- **Active Milestone**: None (Release Readiness complete)
- **Current Issue**: None
- **Current Branch**: main
- **Plugin Version**: 1.3.0

## Progress Log
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Bootstrapped engineering-plugin context files (INSTRUCTIONS.md, MEMORY.md) in unguided mode; REQUIREMENTS.MD already existed and is git-ignored, added a Milestones section to it locally.
- [2026-09-05 19:40] @jameswanga: Unguided housekeeping: skipped self-update / marketplace suggestions this session.

- [2026-09-05 19:42] @jameswanga: Feature Classification (unguided): no milestones existed; created milestone "Release Readiness" (#1) with issues #2 (btleplug 0.13), #3 (init from existing Adapter), #4 (crates.io publishing on v* tags).
- [2026-09-05 19:45] @jameswanga: Issue #2: clarifying-question gate — defaults accepted (handle RssiUpdate by refreshing known probes only; raise rust-version to 1.85 since uuid/getrandom require it). Implemented, tests green, PR #5 opened; review agents launched.

- [2026-09-05 19:52] @jameswanga: PR #5 review: simplicity clean; conventions flagged duplicate CHANGELOG `### Changed` heading (Critical, fixed), stale INSTRUCTIONS.md Deploy text and MEMORY.md note (Important, fixed).

- [2026-09-05 20:02] @jameswanga: PR #5 approved and merged; issue #2 closed. Local branch delete was blocked by the permission classifier (left in place). Architecture refresh deferred to milestone end to avoid regenerating three times in one run.
- [2026-09-05 20:10] @jameswanga: Issue #3: gate defaults accepted (non-async infallible `with_adapter`, shared `from_scanner` path, `adapter()` getter, `pub use btleplug` re-export, `existing_adapter` example). Implemented, tests + doctests green.

- [2026-09-05 20:20] @jameswanga: PR #6 review: dedup BleScanner::new via with_adapter (Important, fixed); documented shared-scan hazard on with_adapter/README/example (Important, fixed); getter doc wording + rustfmt on example (Important, fixed).

- [2026-09-05 20:25] @jameswanga: PR #6 approved and merged; issue #3 closed.
- [2026-09-05 20:35] @jameswanga: Issue #4: gate defaults accepted (release.sh verifies tag==manifest version, idempotent via crates.io lookup, `cargo publish --locked`; workflow on `v*` tags, ubuntu + libdbus-1-dev; .github/release.yml label categories). Fixed Cargo.toml repository URL to jwanga, dropped redundant `readme` key, folded the never-published placeholder `[0.1.0] - 2024-XX-XX` CHANGELOG section into `[Unreleased]` so the first `/release` produces one 0.1.0 heading.

- [2026-09-05 20:45] @jameswanga: PR #7 review: single cargo metadata call, workflow relies on GITHUB_REF_NAME default, created `breaking`/`skip-changelog` labels, excluded MEMORY.md/INSTRUCTIONS.md/release.sh from the crate tarball, corrected workflow header comment (all Important, fixed).

- [2026-09-05 20:50] @jameswanga: PR #7 approved and merged; issue #4 closed. All Release Readiness issues done.
- [2026-09-05 21:00] @jameswanga: Refreshed architecture diagram (trigger: issue-close #2 #3 #4, diagram type: flowchart, coverage-gap: REQUIREMENTS.MD is git-ignored so its Architecture section is local-only)

- [2026-09-05 21:10] @jameswanga: Milestone "Release Readiness" closed on GitHub; REQUIREMENTS.MD line checked and annotated locally (file is git-ignored).
- [2026-09-05 21:15] @jameswanga: Released combustion-rust-ble v0.1.0 — no prior tag, Cargo.toml already stated 0.1.0, so the first tag adopted the manifest version (no bump computed). Tag pushed, GitHub Release created, workflow run 34007699992 published 0.1.0 to crates.io successfully. Release URL: https://github.com/jwanga/combustion-rust-ble/releases/tag/v0.1.0
- [2026-09-05 21:15] @jameswanga: Unguided run ended at milestone boundary (scope=milestone). Known follow-ups: pre-existing `clippy::erasing_op` error in src/protocol/status.rs:401 test code; REQUIREMENTS.MD is git-ignored so milestone tracking there is local-only; stale local branches issue-2/3/4 not deleted (permission classifier blocked `git branch -D`).

## Key Decisions
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Release config inferred as `mode: single`, tag `v{version}`, manifest `Cargo.toml:[package].version`. First tag will adopt the manifest version (0.1.0).

## Notes
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Cargo.toml `repository` pointed at combustion-inc org, not jwanga; corrected in issue #4 (crates.io publishing).
