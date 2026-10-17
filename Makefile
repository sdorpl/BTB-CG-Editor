# Automatyzacja: `make check` uruchamia wszystko, co weryfikuje projekt.
.PHONY: check fmt lint test wasm parity e2e convert-quality ui-shots
check:
	./tools/check.sh
fmt:
	cargo fmt --all
lint:
	cargo clippy -p btb-motion -p btb-motion-wasm --all-targets -- -D warnings
test:
	cargo test --workspace
wasm:
	./tools/build-wasm.sh
parity: wasm
	node tools/wasm-parity.js
e2e: wasm
	cargo test -q --bin btb-cgeditor export_e2e
	cargo build -q
	node tools/e2e-motion.js
	STRICT=1 node tools/e2e-gsap-compare.js
	E2E_SET=e2e-matrix STRICT=1 node tools/e2e-gsap-compare.js
	E2E_SET=e2e-text STRICT=1 node tools/e2e-gsap-compare.js
	STRICT=1 node tools/ease-parity.js
convert-quality:
	STRICT=1 node tools/convert-quality.js

# Paczki binarne (dist/): linux = .deb + AppImage, windows = .exe + .zip, macos = .app.zip + .dmg; native = bieżący system.
.PHONY: package package-linux package-windows package-macos
package:
	./tools/package.sh native
package-linux:
	./tools/package.sh linux
package-windows:
	./tools/package.sh windows
package-macos:
	./tools/package.sh macos

ui-shots:
	python3 tools/ui-shots.py
