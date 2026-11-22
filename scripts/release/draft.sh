#!/usr/bin/env bash
set -euo pipefail

tagged="$(gh api "repos/$GITHUB_REPOSITORY/commits/refs/tags/$GITHUB_REF_NAME" --jq .sha)"
if [[ "$tagged" != "$GITHUB_SHA" ]]; then
  echo "::error::$GITHUB_REF_NAME no longer points at the commit this run built"
  exit 1
fi
version="${GITHUB_REF_NAME#v}"
prerelease=()
[[ "$version" == *-* ]] && prerelease=(--prerelease)
gh release create "$GITHUB_REF_NAME" "$RELEASE_DMG" ${prerelease[@]+"${prerelease[@]}"} \
  --repo "$GITHUB_REPOSITORY" --verify-tag --draft --generate-notes \
  --title "DockMoss $version"
