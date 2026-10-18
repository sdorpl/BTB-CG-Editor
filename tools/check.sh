#!/usr/bin/env bash
# Pełna weryfikacja projektu: formatowanie, lint nowych crate'ów, testy, WASM + zgodność w Node.
set -euo pipefail
cd "$(dirname "$0")/.."
step() { printf '\n\033[1;34m== %s\033[0m\n' "$*"; }

step "fmt (btb-motion, btb-motion-wasm)"
cargo fmt -p btb-motion -p btb-motion-wasm -- --check || { echo "Uruchom: cargo fmt --all"; exit 1; }

step "clippy (-D warnings)"
cargo clippy -p btb-motion -p btb-motion-wasm --all-targets -- -D warnings

step "testy (workspace)"
cargo test --workspace

if rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown && command -v node >/dev/null; then
  step "WASM build + zgodność z wersją natywną (node)"
  ./tools/build-wasm.sh
  node tools/wasm-parity.js
  BTB_CG_DIR="${BTB_CG_DIR:-$HOME/Nextcloud/GitHub/BTB-CG-System-alpha}" STRICT=1 node tools/ease-parity.js
else
  step "WASM pominięty (brak targetu wasm32-unknown-unknown lub node)"
fi
BTB_CG="${BTB_CG_DIR:-$HOME/Nextcloud/GitHub/BTB-CG-System-alpha}"
if command -v node >/dev/null && [ -d "$BTB_CG/node_modules/playwright" ]; then
  step "e2e w Chromium: strona wyjściowa + zgodność z GSAP (BTB-CG: $BTB_CG)"
  cargo build -q
  BTB_CG_DIR="$BTB_CG" node tools/e2e-motion.js
  BTB_CG_DIR="$BTB_CG" node tools/e2e-fit.js
  BTB_CG_DIR="$BTB_CG" node tools/e2e-fonts.js
  BTB_CG_DIR="$BTB_CG" STRICT=1 node tools/e2e-gsap-compare.js
  BTB_CG_DIR="$BTB_CG" E2E_SET=e2e-matrix STRICT=1 node tools/e2e-gsap-compare.js
  BTB_CG_DIR="$BTB_CG" E2E_SET=e2e-text STRICT=1 node tools/e2e-gsap-compare.js
  if [ -f "$BTB_CG/db.json" ]; then
    step "jakość konwersji ręcznych szablonów (piksele + intencja animacji)"
    BTB_CG_DIR="$BTB_CG" STRICT=1 node tools/convert-quality.js | tail -4
  fi
else
  step "e2e w przeglądarce pominięte (brak node lub playwright w $BTB_CG; ustaw BTB_CG_DIR)"
fi
printf '\n\033[1;32mOK\033[0m\n'
