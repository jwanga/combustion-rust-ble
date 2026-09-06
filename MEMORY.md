# Project Memory

## Current State
- **Active Milestone**: Release Readiness
- **Current Issue**: #2 Bump btleplug to 0.13 (PR #5 open)
- **Current Branch**: issue-2-bump-btleplug
- **Plugin Version**: 1.3.0

## Progress Log
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Bootstrapped engineering-plugin context files (INSTRUCTIONS.md, MEMORY.md) in unguided mode; REQUIREMENTS.MD already existed and is git-ignored, added a Milestones section to it locally.
- [2026-09-05 19:40] @jameswanga: Unguided housekeeping: skipped self-update / marketplace suggestions this session.

- [2026-09-05 19:42] @jameswanga: Feature Classification (unguided): no milestones existed; created milestone "Release Readiness" (#1) with issues #2 (btleplug 0.13), #3 (init from existing Adapter), #4 (crates.io publishing on v* tags).
- [2026-09-05 19:45] @jameswanga: Issue #2: clarifying-question gate — defaults accepted (handle RssiUpdate by refreshing known probes only; raise rust-version to 1.85 since uuid/getrandom require it). Implemented, tests green, PR #5 opened; review agents launched.

- [2026-09-05 19:52] @jameswanga: PR #5 review: simplicity clean; conventions flagged duplicate CHANGELOG `### Changed` heading (Critical, fixed), stale INSTRUCTIONS.md Deploy text and MEMORY.md note (Important, fixed).

## Key Decisions
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Release config inferred as `mode: single`, tag `v{version}`, manifest `Cargo.toml:[package].version`. First tag will adopt the manifest version (0.1.0).

## Notes
<!-- Each entry MUST use the format: [YYYY-MM-DD HH:MM] @username: description -->
- [2026-09-05 19:40] @jameswanga: Cargo.toml `repository` pointed at combustion-inc org, not jwanga; to be corrected in issue #4 (crates.io publishing).
