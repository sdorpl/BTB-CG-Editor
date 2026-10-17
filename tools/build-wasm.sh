#!/usr/bin/env bash
# Buduje btb_motion_wasm.wasm i generuje dane referencyjne (natywne) do testu zgodności.
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
cargo build -p btb-motion-wasm --release --target wasm32-unknown-unknown
OUT="$TARGET_DIR/parity"
rm -rf "$OUT"
# dodatkowe fixtures (np. wygenerowane przez testy aplikacji) trafiają do $OUT/extra/*.json
EXTRA=()
if [ -d "$TARGET_DIR/parity-extra" ]; then EXTRA=("$TARGET_DIR"/parity-extra/*.json); fi
cargo run -q -p btb-motion-wasm --example dump -- "$OUT" "${EXTRA[@]}"
echo "$TARGET_DIR/wasm32-unknown-unknown/release/btb_motion_wasm.wasm" > "$OUT/wasm-path.txt"
cp "$TARGET_DIR/wasm32-unknown-unknown/release/btb_motion_wasm.wasm" assets/btb_motion_wasm.wasm
ls -l assets/btb_motion_wasm.wasm
