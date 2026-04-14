#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

PHRASE="${1:-hello this is a voice test please transcribe this sentence clearly}"

printf '\n[1/2] Backend compile check\n'
if ! cargo check --manifest-path "$ROOT/src-tauri/Cargo.toml" --bin cohostd >/dev/null 2>&1; then
  echo "FAIL: cohostd failed to compile"
  exit 1
fi
echo "PASS: cohostd compiled"

printf '\n[2/2] Headless voice smoke through cohostd\n'
OUTPUT_FILE="$TMP_DIR/voice-smoke.txt"
if ! timeout 60s cargo run --quiet --manifest-path "$ROOT/src-tauri/Cargo.toml" --bin cohostd -- worker voice-smoke "$PHRASE" > "$OUTPUT_FILE"; then
  echo "FAIL: cohostd headless voice smoke timed out or failed"
  cat "$OUTPUT_FILE" 2>/dev/null || true
  exit 1
fi

RAW_OUTPUT="$(tr -d '\r\n' < "$OUTPUT_FILE")"
echo "$RAW_OUTPUT"
if ! grep -q '"ok":true' <<< "$RAW_OUTPUT"; then
  echo "FAIL: cohostd voice smoke returned a failure envelope"
  exit 1
fi

echo "PASS: cohostd headless voice smoke succeeded"
