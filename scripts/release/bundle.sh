#!/usr/bin/env bash
set -euo pipefail

# Start the Tauri CLI with node, not through pnpm. pnpm puts node_modules/.bin
# first on PATH, so a dependency's bin named codesign, security or xcrun would
# run here with the keychain unlocked; pnpm would also run any pretauri or
# posttauri script. The null drops a beforeBundleCommand from tauri.conf.json
# or a tauri.<platform>.conf.json, which would run here too.
exec node node_modules/@tauri-apps/cli/tauri.js bundle --bundles app,dmg \
  --config '{"build":{"beforeBundleCommand":null}}'
