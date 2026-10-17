#!/usr/bin/env bash
# Składa katalog `runtime/` dołączany do paczek: Node.js, pakiety konwertera BTB-CG (Playwright, GSAP, Handlebars)
# oraz przeglądarkę Chromium dla Playwrighta. Dzięki temu import ręcznych szablonów działa bez instalacji Node.
#   tools/make-runtime.sh <linux|windows|macos> [wyjście]    (uruchamiać na systemie docelowym)
# Wynik: dist/.cache/runtime-<os>-<arch> (lub podany katalog). Zmienne: NODE_VERSION.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
OS="${1:?Użycie: $0 <linux|windows|macos> [wyjście]}"
NODE_VERSION="${NODE_VERSION:-22.22.2}"
case "$(uname -m)" in x86_64|amd64) ARCH=x64 ;; aarch64|arm64) ARCH=arm64 ;; *) ARCH="$(uname -m)" ;; esac
OUT="${2:-$ROOT/dist/.cache/runtime-$OS-$ARCH}"
step() { printf '\n\033[1;34m== %s\033[0m\n' "$*" >&2; }

if [ -f "$OUT/.complete" ]; then echo "$OUT"; exit 0; fi
rm -rf "$OUT"; mkdir -p "$OUT/node" "$OUT/btb-cg" "$OUT/browsers"
DL="$(mktemp -d)"; trap 'rm -rf "$DL"' EXIT

step "Node.js $NODE_VERSION ($OS-$ARCH)"
BASE="https://nodejs.org/dist/v$NODE_VERSION"
case "$OS" in
  linux)   PKG="node-v$NODE_VERSION-linux-$ARCH.tar.xz" ;;
  macos)   PKG="node-v$NODE_VERSION-darwin-$ARCH.tar.gz" ;;
  windows) PKG="node-v$NODE_VERSION-win-$ARCH.zip" ;;
  *) echo "Nieznany system: $OS" >&2; exit 2 ;;
esac
curl -fL --retry 3 -o "$DL/$PKG" "$BASE/$PKG"
curl -fL --retry 3 -o "$DL/SHASUMS256.txt" "$BASE/SHASUMS256.txt"
EXPECT="$(grep " $PKG\$" "$DL/SHASUMS256.txt" | cut -d' ' -f1)"
if command -v sha256sum >/dev/null; then ACTUAL="$(sha256sum "$DL/$PKG" | cut -d' ' -f1)"; else ACTUAL="$(shasum -a 256 "$DL/$PKG" | cut -d' ' -f1)"; fi
[ -n "$EXPECT" ] && [ "$EXPECT" = "$ACTUAL" ] || { echo "Suma kontrolna Node się nie zgadza ($PKG)" >&2; exit 1; }
mkdir "$DL/x"
case "$OS" in
  windows) powershell -NoProfile -Command "Expand-Archive -Path '$(cygpath -w "$DL/$PKG")' -DestinationPath '$(cygpath -w "$DL/x")'"; cp "$DL"/x/*/node.exe "$OUT/node/node.exe"; cp "$DL"/x/*/LICENSE "$OUT/node/" ;;
  *) tar -xf "$DL/$PKG" -C "$DL/x"; install -Dm755 "$DL"/x/*/bin/node "$OUT/node/bin/node"; cp "$DL"/x/*/LICENSE "$OUT/node/" ;;
esac
NODE="$OUT/node/bin/node"; [ "$OS" = windows ] && NODE="$OUT/node/node.exe"
# npm jest tylko narzędziem budowy: bierzemy z Node'a pobranego w tym kroku
NPM_CLI="$(ls -d "$DL"/x/*/lib/node_modules/npm/bin/npm-cli.js "$DL"/x/*/node_modules/npm/bin/npm-cli.js 2>/dev/null | head -1 || true)"

step "Pakiety konwertera (vendor/btb-cg)"
cp vendor/btb-cg/*.js vendor/btb-cg/package.json "$OUT/btb-cg/"
(cd "$OUT/btb-cg" && "$NODE" "$NPM_CLI" install --omit=dev --no-audit --no-fund --loglevel=error)

step "Przeglądarka Chromium dla Playwrighta"
(cd "$OUT/btb-cg" && PLAYWRIGHT_BROWSERS_PATH="$OUT/browsers" "$NODE" node_modules/playwright/cli.js install chromium)

touch "$OUT/.complete"
echo "$OUT"
