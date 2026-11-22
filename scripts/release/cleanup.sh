#!/usr/bin/env bash
set -euo pipefail

# Runs even when an earlier step failed, so either value may be unset.
[[ -n "${RELEASE_KEYCHAIN:-}" ]] && security delete-keychain "$RELEASE_KEYCHAIN" || true
[[ -n "${APPLE_API_KEY_PATH:-}" ]] && rm -f "$APPLE_API_KEY_PATH" || true
