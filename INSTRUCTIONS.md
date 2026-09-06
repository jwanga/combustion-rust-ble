# Project Instructions

## General
- `combustion-rust-ble` is a published-library crate. Public API changes must be recorded in `CHANGELOG.md` under `[Unreleased]`.
- Prefer Conventional Commits subjects (`feat:`, `fix:`, `feat!:` for breaking) so `/release` can infer the SemVer bump.
- `REQUIREMENTS.MD` is git-ignored in this repo (it is a large local spec); milestone tracking lives on GitHub milestones.

## Build
cargo build

## Test
cargo test

## Run
cargo run --example discover_probes

## Deploy
Publishing to crates.io is automated: pushing a `v*` tag triggers `.github/workflows/release.yml`, which runs `release.sh` with the `CARGO_REGISTRY_TOKEN` repository secret.

## Releases
mode: single
trigger: milestone
components:
  - name: combustion-rust-ble
    path: .
    tag: v{version}
    manifests:
      - Cargo.toml:[package].version

## Overrides
- None.
