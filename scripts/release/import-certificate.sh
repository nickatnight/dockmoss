#!/usr/bin/env bash
set -euo pipefail

# Given APPLE_CERTIFICATE, Tauri would import it and sign at once. Importing
# it here lets this step refuse the wrong certificate before anything is
# signed; Tauri then finds the identity through the keychain search list.
if [[ ! "${SIGN_TIMEOUT_MINUTES:-}" =~ ^[1-9][0-9]*$ ]]; then
  echo "::error::SIGN_TIMEOUT_MINUTES must be the sign job's timeout in whole minutes"
  exit 1
fi
umask 077
kc="$RUNNER_TEMP/release.keychain-db"
p12="$RUNNER_TEMP/certificate.p12"
trap 'rm -f "$p12"' EXIT
kc_password="$(openssl rand -hex 32)"
echo "RELEASE_KEYCHAIN=$kc" >> "$GITHUB_ENV"
printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$p12"
security create-keychain -p "$kc_password" "$kc"
security set-keychain-settings -lut "$((SIGN_TIMEOUT_MINUTES * 60))" "$kc"
security unlock-keychain -p "$kc_password" "$kc"
security import "$p12" -k "$kc" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -x
rm -f "$p12"
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kc_password" "$kc" > /dev/null
keychains=()
while IFS= read -r line; do
  keychains+=("$line")
done < <(security list-keychains -d user | sed 's/^ *"//; s/"$//')
security list-keychains -d user -s "$kc" ${keychains[@]+"${keychains[@]}"}

# The identity names the developer, and this log is public: compare it, never
# print it. The Team ID survives the move from an individual to an
# organization membership, so only the name refuses a certificate from the
# wrong one.
expected="Developer ID Application: $DEVELOPER_NAME ($TEAM_ID)"
found="$(security find-identity -v -p codesigning "$kc" \
  | sed -n 's/^ *[0-9][0-9]*) \([0-9A-F]\{40\}\) "\(.*\)"$/\1 \2/p')"
count=0
[[ -z "$found" ]] || count="$(grep -c '' <<< "$found")"
if [[ $count -ne 1 ]]; then
  echo "::error::found $count valid signing identities; the certificate must hold exactly one, Developer ID Application, matching secrets.APPLE_DEVELOPER_NAME and vars.APPLE_TEAM_ID"
  exit 1
fi
name="${found#* }"
if [[ "$name" != "$expected" ]]; then
  reasons=""
  [[ "$name" == "Developer ID Application: "* ]] || reasons="$reasons; it is not a Developer ID Application identity"
  [[ "$name" == *" ($TEAM_ID)" ]] || reasons="$reasons; its Team ID is not vars.APPLE_TEAM_ID"
  [[ -n "$reasons" ]] || reasons="; its name differs from secrets.APPLE_DEVELOPER_NAME"
  echo "::error::the certificate's one valid signing identity does not match${reasons/#;/:}"
  exit 1
fi
# Sign by the certificate's SHA-1: codesign then uses exactly this
# certificate, and Tauri's "Signing with identity" lines show the hash.
echo "APPLE_SIGNING_IDENTITY=${found%% *}" >> "$GITHUB_ENV"
