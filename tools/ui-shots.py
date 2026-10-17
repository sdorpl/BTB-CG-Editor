#!/usr/bin/env python3
"""Testy wizualne edytora: uruchamia `btb-cgeditor --open <fixture> --shot ...` dla scenariuszy z
tests/ui/scenarios.json i porównuje zrzut piksel po pikselu ze wzorcem tests/ui/golden/<nazwa>.png.

  python3 tools/ui-shots.py              # porównanie (kod wyjścia 1 przy różnicach)
  python3 tools/ui-shots.py --update     # zapis/odświeżenie wzorców
  python3 tools/ui-shots.py idle hold    # tylko wybrane scenariusze

Wymaga wyświetlacza (okno otwiera się na chwilę) i Pillow. Wzorce zależą od rozmiaru okna, czcionek
i skali ekranu, więc generuj je na tej samej maszynie, na której porównujesz. Różnice zapisują się
w $UI_SHOTS_OUT (domyślnie /tmp/btb-ui-shots) jako <nazwa>.actual.png i <nazwa>.diff.png.
"""
import json, os, subprocess, sys
from pathlib import Path
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parent.parent
CFG = json.loads((ROOT / "tests/ui/scenarios.json").read_text())
GOLDEN = ROOT / "tests/ui/golden"
OUT = Path(os.environ.get("UI_SHOTS_OUT", "/tmp/btb-ui-shots"))


def binary():
    target = os.environ.get("CARGO_TARGET_DIR", "/tmp/btb-cgeditor-target")
    subprocess.run(["cargo", "build", "-q"], cwd=ROOT, check=True)
    return Path(target) / "debug" / "btb-cgeditor"


def shoot(exe, sc, dest):
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        dest.unlink()
    cmd = [str(exe), "--open", str(ROOT / CFG["fixture"]), "--shot", str(dest), *sc["args"]]
    subprocess.run(cmd, cwd=ROOT, check=True, timeout=120, env={**os.environ, "BTB_LANG": "pl"})
    if not dest.exists():
        raise RuntimeError("brak zrzutu")


def compare(actual, golden, tol):
    a, g = Image.open(actual).convert("RGB"), Image.open(golden).convert("RGB")
    if a.size != g.size:
        return 1.0, None, f"rozmiar {a.size} != {g.size}"
    diff = ImageChops.difference(a, g).convert("L")
    for x0, y0, x1, y1 in CFG.get("ignore_bottom", []) and [(0, a.height - CFG["ignore_bottom"], a.width, a.height)]:
        diff.paste(0, (x0, y0, x1, y1))  # pasek statusu zawiera ścieżkę pliku
    bad = sum(1 for p in diff.get_flattened_data() if p > tol["pixel"]) if hasattr(diff, "get_flattened_data") else sum(1 for p in diff.getdata() if p > tol["pixel"])
    ratio = bad / (a.width * a.height)
    return ratio, diff.point(lambda p: 255 if p > tol["pixel"] else 0), f"{bad} px ({ratio:.3%})"


def main():
    args = sys.argv[1:]
    update = "--update" in args
    names = [a for a in args if not a.startswith("--")]
    scenarios = [s for s in CFG["scenarios"] if not names or s["name"] in names]
    exe = binary()
    failed = []
    for sc in scenarios:
        name = sc["name"]
        golden = GOLDEN / f"{name}.png"
        if update:
            shoot(exe, sc, golden)
            print(f"ZAPISANO  {name}")
            continue
        actual = OUT / f"{name}.actual.png"
        shoot(exe, sc, actual)
        if not golden.exists():
            print(f"BRAK WZORCA  {name} (uruchom z --update)")
            failed.append(name)
            continue
        ratio, mask, info = compare(actual, golden, CFG["tolerance"])
        if ratio > CFG["tolerance"]["max_diff_ratio"]:
            if mask is not None:
                mask.save(OUT / f"{name}.diff.png")
            print(f"RÓŻNICA  {name}: {info} -> {OUT}/{name}.diff.png")
            failed.append(name)
        else:
            print(f"OK        {name}: {info}")
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
