#!/usr/bin/env bash
set -euo pipefail

# The tag must be "v" followed by the version in all three version files, and
# it must be on a commit on main.
conf="$(jq -r .version src-tauri/tauri.conf.json)"
pkg="$(jq -r .version package.json)"
cargo="$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -n 1)"
echo "tag $GITHUB_REF_NAME, tauri.conf.json $conf, package.json $pkg, Cargo.toml $cargo"
if [[ "$GITHUB_REF_NAME" != "v$conf" || "$pkg" != "$conf" || "$cargo" != "$conf" ]]; then
  echo "::error::the tag and the three version fields must all match"
  exit 1
fi
if ! git rev-parse --verify --quiet origin/main > /dev/null; then
  echo "::error::origin/main is missing; the checkout needs fetch-depth: 0"
  exit 1
fi
if ! git merge-base --is-ancestor "$GITHUB_SHA" origin/main; then
  echo "::error::the tagged commit is not on main"
  exit 1
fi
