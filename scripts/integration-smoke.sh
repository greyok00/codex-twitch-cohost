#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"

echo "[1/5] Frontend type/lint check"
npm run check

echo "[2/5] Frontend production build"
npm run build

echo "[3/5] Rust compile checks"
cargo check --manifest-path src-tauri/Cargo.toml --all-targets

echo "[4/5] Config/resource sanity"
[ -f config.example.json ]
[ -f personality.example.json ]
[ -f src-tauri/tauri.conf.json ]

echo "[5/5] Dev boot smoke (tauri dev, bounded)"
LOG_FILE="$(mktemp)"
set +e
timeout 70s bash -lc 'cd "$0" && PATH="$HOME/.cargo/bin:$PATH" npm run tauri dev' "$ROOT_DIR" >"$LOG_FILE" 2>&1
RC=$?
set -e
if ! grep -Fq 'target/debug/twitch-cohost-bot' "$LOG_FILE"; then
  echo "Dev boot smoke failed. Output:"
  cat "$LOG_FILE"
  rm -f "$LOG_FILE"
  exit 1
fi
if [ "$RC" -ne 0 ] && [ "$RC" -ne 124 ]; then
  echo "tauri dev exited unexpectedly (code $RC). Output:"
  cat "$LOG_FILE"
  rm -f "$LOG_FILE"
  exit 1
fi
rm -f "$LOG_FILE"

echo "Smoke test passed."
