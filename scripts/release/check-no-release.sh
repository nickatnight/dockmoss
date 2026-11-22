#!/usr/bin/env bash
set -euo pipefail

# gh release create would add a second draft beside an existing one.
tags="$(gh release list --repo "$GITHUB_REPOSITORY" --limit 1000 --json tagName --jq '.[].tagName')"
if grep -qxF "$GITHUB_REF_NAME" <<< "$tags"; then
  echo "::error::a release for $GITHUB_REF_NAME already exists, drafts included; delete it, then re-run"
  exit 1
fi
