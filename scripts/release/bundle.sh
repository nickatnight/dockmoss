#!/usr/bin/env bash
set -euo pipefail

# Start the Tauri CLI with node, not through pnpm. pnpm puts node_modules/.bin
# first on PATH, so a dependency's bin named codesign, security or xcrun would
# run here with the keychain unlocked; pnpm would also run any pretauri or
# posttauri script. The nulls drop a beforeBundleCommand (either spelling) from
# tauri.conf.json, Tauri.toml or a platform file, which would run here too.
# Tauri's binding loader would first require @tauri-apps/cli-darwin-universal,
# which any package can plant in pnpm's hoisted node_modules, so name the
# binding's file. If that file fails to load, the loader falls back to another
# hoisted name, so check it is there first. NAPI_RS_FORCE_WASI would send the
# loader to that name even after the binding loads, so unset it.
cli="$(cd node_modules/@tauri-apps/cli && pwd -P)"
binding="$(dirname "$cli")/cli-darwin-arm64/cli.darwin-arm64.node"
if [[ ! -f "$binding" || -L "$binding" ]]; then
  echo "::error::Tauri's arm64 binding is not beside the CLI"
  exit 1
fi
unset NAPI_RS_FORCE_WASI
NAPI_RS_NATIVE_LIBRARY_PATH="$binding" exec node "$cli/tauri.js" bundle --bundles app,dmg \
  --config '{"build":{"beforeBundleCommand":null,"before-bundle-command":null}}'
