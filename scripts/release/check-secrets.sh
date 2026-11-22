#!/usr/bin/env bash
set -euo pipefail

missing=""
for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_API_KEY \
            APPLE_API_ISSUER APPLE_API_PRIVATE_KEY TEAM_ID DEVELOPER_NAME; do
  [[ -n "${!name:-}" ]] || missing="$missing $name"
done
if [[ -n "$missing" ]]; then
  echo "::error::not set in the release environment:$missing (TEAM_ID is vars.APPLE_TEAM_ID, DEVELOPER_NAME is secrets.APPLE_DEVELOPER_NAME)"
  exit 1
fi
