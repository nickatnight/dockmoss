#!/usr/bin/env bash
set -euo pipefail

umask 077
key="$RUNNER_TEMP/AuthKey_$APPLE_API_KEY.p8"
printf '%s\n' "$APPLE_API_PRIVATE_KEY" > "$key"
echo "APPLE_API_KEY_PATH=$key" >> "$GITHUB_ENV"
