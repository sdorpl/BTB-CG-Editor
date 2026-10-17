#!/usr/bin/env bash
# Kopiuje runtime WASM (odtwarzacz + silnik) do BTB-CG (`vendor/btb-motion/`), skąd ładuje go output.html (plan M2.2).
# Uruchom po zmianie `assets/btb-motion-player.js` albo po `make wasm`. Zapisuje sumy kontrolne w SHA256SUMS.
set -euo pipefail
cd "$(dirname "$0")/.."
DST="${BTB_CG_DIR:-$HOME/Nextcloud/GitHub/BTB-CG-System-alpha}/vendor/btb-motion"
mkdir -p "$DST"
cp assets/btb-motion-player.js assets/btb_motion_wasm.wasm "$DST/"
(cd "$DST" && sha256sum btb-motion-player.js btb_motion_wasm.wasm > SHA256SUMS)
echo "Skopiowano do $DST"; cat "$DST/SHA256SUMS"
