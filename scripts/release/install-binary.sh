#!/usr/bin/env bash
set -euo pipefail

# Take only the one regular file build uploaded. The artifact zip drops the
# mode bits, so install sets them.
bin="$RUNNER_TEMP/binary/dockmoss"
entries="$(find "$RUNNER_TEMP/binary" -mindepth 1 | wc -l | tr -d ' ')"
if [[ "$entries" -ne 1 || ! -f "$bin" || -L "$bin" ]]; then
  echo "::error::the binary artifact must hold exactly one regular file, dockmoss"
  exit 1
fi
mkdir -p src-tauri/target/release
install -m 0755 "$bin" src-tauri/target/release/dockmoss
