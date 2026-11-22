#!/usr/bin/env bash
set -euo pipefail

# The designated requirement must name the bundle identifier and the Team ID,
# not a cdhash: that is what lets the Accessibility grant and the login item
# survive an update.
identifier="$(jq -r .identifier src-tauri/tauri.conf.json)"
app="src-tauri/target/release/bundle/macos/$(jq -r .productName src-tauri/tauri.conf.json).app"
fail() { echo "::error::$1"; exit 1; }

# Neither codesign nor spctl refuses a main executable that can't run, and the
# binary crossed from build in a zip.
exe="$(plutil -extract CFBundleExecutable raw "$app/Contents/Info.plist")" \
  || fail "the app's Info.plist names no CFBundleExecutable"
[[ -f "$app/Contents/MacOS/$exe" && -x "$app/Contents/MacOS/$exe" ]] \
  || fail "the app's main executable is not an executable file"

codesign --verify --deep --strict --verbose=2 "$app"
info="$(codesign -dv --verbose=2 "$app" 2>&1)"
grep -qxF "Identifier=$identifier" <<< "$info" || fail "the app's identifier is not $identifier"
grep -qxF "TeamIdentifier=$TEAM_ID" <<< "$info" || fail "the app is not signed by team $TEAM_ID"
grep -qE '^CodeDirectory .*flags=.*runtime' <<< "$info" || fail "the app lacks the hardened runtime"
dmg_info="$(codesign -dv --verbose=2 "$RELEASE_DMG" 2>&1)"
grep -qxF "TeamIdentifier=$TEAM_ID" <<< "$dmg_info" || fail "the DMG is not signed by team $TEAM_ID"

requirement="$(codesign -d -r- "$app" 2>&1)"
grep -qF "identifier \"$identifier\"" <<< "$requirement" \
  && grep -qE "certificate leaf\[subject\.OU\] = \"?$TEAM_ID\"?( |\)|$)" <<< "$requirement" \
  || fail "the designated requirement does not name the identifier and the Team ID"

xcrun stapler validate "$app"
xcrun stapler validate "$RELEASE_DMG"

gatekeeper() {
  local what="$1"
  local out="$RUNNER_TEMP/spctl-$what.txt" rc=0
  shift
  spctl --assess -vv "$@" > "$out" 2>&1 || rc=$?
  # spctl's origin= line names the developer: print only the verdict
  # and the source.
  grep -E ': (accepted|rejected)$|^source=' "$out" || true
  [[ $rc -eq 0 ]] && grep -qx 'source=Notarized Developer ID' "$out" \
    || fail "Gatekeeper does not accept the $what as notarized"
}
gatekeeper app --type execute "$app"
gatekeeper DMG --type open --context context:primary-signature "$RELEASE_DMG"
