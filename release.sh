#!/usr/bin/env bash
# Publish this crate to crates.io for a release tag.
#
# Usage: ./release.sh [vX.Y.Z]
#
# The tag defaults to $GITHUB_REF_NAME (set by GitHub Actions) or the tag that
# points at HEAD. The tag version must match Cargo.toml's [package] version;
# this script never rewrites the manifest — the /release skill already did
# that in the commit the tag points at.
#
# Requires CARGO_REGISTRY_TOKEN in the environment. Safe to re-run: if this
# version is already on crates.io the script exits 0 without publishing.
set -euo pipefail

die() { echo "release.sh: $*" >&2; exit 1; }

tag="${1:-${GITHUB_REF_NAME:-$(git describe --tags --exact-match 2>/dev/null || true)}}"
[[ -n "$tag" ]] || die "no tag given and HEAD is not tagged"
[[ "$tag" == v* ]] || die "tag '$tag' does not start with 'v'"
tag_version="${tag#v}"

crate="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].name')"
manifest_version="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')"
[[ "$tag_version" == "$manifest_version" ]] \
  || die "tag version $tag_version does not match Cargo.toml version $manifest_version"

: "${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN is not set}"

# Idempotency: crates.io versions are immutable, so a re-run after a partial
# failure must not try to publish the same version twice.
published="$(curl -fsSL -H 'User-Agent: combustion-rust-ble release.sh' \
  "https://crates.io/api/v1/crates/${crate}/${tag_version}" 2>/dev/null \
  | jq -r '.version.num // empty' || true)"
if [[ "$published" == "$tag_version" ]]; then
  echo "${crate} ${tag_version} is already on crates.io; nothing to do."
  exit 0
fi

echo "Publishing ${crate} ${tag_version} to crates.io"
cargo publish --locked
