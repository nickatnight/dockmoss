#!/usr/bin/env bash
set -euo pipefail

[[ -n "${RELEASE_KEYCHAIN:-}" ]] && security delete-keychain "$RELEASE_KEYCHAIN" || true
[[ -n "${APPLE_API_KEY_PATH:-}" ]] && rm -f "$APPLE_API_KEY_PATH" || true
