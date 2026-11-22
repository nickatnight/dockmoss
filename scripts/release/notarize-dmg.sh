#!/usr/bin/env bash
set -euo pipefail

# tauri bundle has already signed the DMG with APPLE_SIGNING_IDENTITY, and
# codesign gives a Developer ID signature a secure timestamp by default.
shopt -s nullglob
dmgs=(src-tauri/target/release/bundle/dmg/*.dmg)
if [[ ${#dmgs[@]} -ne 1 ]]; then
  echo "::error::expected one DMG, found ${#dmgs[@]}"
  exit 1
fi
dmg="${dmgs[0]}"
auth=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
# Fetch the log whatever notarytool's exit status, so a rejection shows why.
result="$(xcrun notarytool submit "$dmg" --wait --output-format json "${auth[@]}")" || true
status="$(jq -r '.status // empty' <<< "$result" 2> /dev/null)" || true
if [[ "$status" != "Accepted" ]]; then
  echo "::error::notarization of the DMG ended as ${status:-no result}"
  id="$(jq -r '.id // empty' <<< "$result" 2> /dev/null)" || true
  [[ -z "$id" ]] || xcrun notarytool log "$id" "${auth[@]}" || true
  exit 1
fi
# The ticket can take a moment to reach Apple's CDN after Accepted.
for attempt in 1 2 3; do
  xcrun stapler staple "$dmg" && break
  [[ $attempt -lt 3 ]] || exit 1
  sleep 30
done
echo "RELEASE_DMG=$dmg" >> "$GITHUB_ENV"
