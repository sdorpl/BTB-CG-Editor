#!/usr/bin/env bash
# Kopiuje do vendor/btb-cg/ pliki BTB-CG potrzebne konwerterowi (wersjonowane razem z edytorem, żeby paczki
# i CI nie zależały od osobnego drzewa BTB-CG). Uruchom ponownie po zmianach w BTB-CG.
set -euo pipefail
cd "$(dirname "$0")/.."
SRC="${BTB_CG_DIR:-$HOME/Nextcloud/GitHub/BTB-CG-System-alpha}"
for f in cgmotion.js element-model.js scene-model.js renderer.js scene_convert.js; do
  [ -f "$SRC/$f" ] || { echo "Brak $SRC/$f" >&2; exit 1; }
  cp "$SRC/$f" vendor/btb-cg/
done
echo "Skopiowano z $SRC"
