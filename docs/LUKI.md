# Luki i braki

> **Ostatni przegląd:** 2026-10-17. Dokument dla agentów i współpracowników wybierających zadanie. Zbiera
> znane braki z `CLAUDE.md` („Znane ograniczenia”), [PROPOZYCJE-FUNKCJI.md](PROPOZYCJE-FUNKCJI.md) i
> [PLAN-btb-motion.md](PLAN-btb-motion.md), uszeregowane. Ocena wagi to osąd, nie pomiar. Aktualizuj, gdy zmieni się
> stan (zamknięta luka usuwa wiersz, nowa dostaje numer).

Kolejność ważności: **G1 niezawodność → G2 wierność wyjścia → G3 praca z plikami → G4 braki funkcji**. Niezawodność
i wierność idą przed nowymi funkcjami.

## G1. Niezawodność (nie wywalaj aplikacji)

| # | Luka | Dowód / uwagi |
|---|---|---|
| 1 | Użycia `unwrap()`/`expect()` w kodzie nietestowym | po przeglądzie 2026-10-17 zostało ok. 22 trafień poza testami: `src/main.rs` (15, narzędzia CLI), `src/presets.rs` (7, warstwy budowane lokalnie ze stałych), `src/i18n.rs` (1, `panic!` przy uszkodzonych wkompilowanych tłumaczeniach). Usunięte: mutexy, `scene`, klucze, `frame` w stronie wyjściowej |
| 2 | Brak ostatniej linii obrony (`catch_unwind` przy otwieraniu, imporcie, eksporcie i zapisie) | zaprojektować: panika → komunikat i zachowanie dokumentu |
| 3 | Brak lintów `clippy::unwrap_used` itd. | dodać `#![deny(...)]` do czystych modułów, nowe moduły startują z nim; `make lint` obejmuje tylko `btb-motion*` |
| 4 | Granica WASM (`btb-motion-wasm`) | `unsafe extern "C"` przyjmuje wskaźniki i długości z hosta; dodać walidację i testy złośliwych danych |
| 5 | Wejścia: JSON elementu, paczka `.cgpack`, obrazy | NaN/inf w czasach i wartościach, ogromne obrazy, głęboko zagnieżdżone dane — testy fuzz-podobne na własnym LCG |

## G2. Wierność wyjścia

| # | Luka | Dowód / uwagi |
|---|---|---|
| 6 | Strona wyjściowa sprawdzona tylko w Chromium (Playwright), nie w samym OBS | wymaga próby w OBS Browser Source |
| 7 | Brak Local Stop na stronie wyjściowej; czas Stop liczony z tekstu domyślnego | punkt J z propozycji |
| 8 | Podgląd egui przybliża efekty tekstowe i maski (`Transform::approx`) | dokładny wygląd daje strona wyjściowa |
| 9 | Auto-szerokość po imporcie: elementy mają stały rozmiar tła | 57–84% zgodnych pikseli przy `DYN=…`; wymaga zmiany modelu współdzielonego z BTB-CG |
| 10 | Layout (`side`, `margin`, `scale`, `rotation`) bez porównania z `renderer.js` | dodać test zgodności |
| 11 | Klucze i ścieżka bez odpowiednika GSAP/`compile.rs` | działają tylko na stronie z WASM; testy `tools/e2e-gsap-compare.js` je pomijają |

## G3. Praca z plikami

| # | Luka | Dowód / uwagi |
|---|---|---|
| 12 | Paczka `.cgpack` zaprojektowana, nie zaimplementowana | [FORMAT-PACZKI.md](FORMAT-PACZKI.md) |
| 13 | Obrazy: brak przeciągania z systemu, wklejania ze schowka, wysyłki na serwer/do OBS | punkt A, sekcja 3 propozycji |
| 14 | Synchronizacja `reference/` (≈1100 plików) przez Nextcloud | dodać do wyłączeń synchronizacji, jeśli przeszkadza |

## G4. Braki funkcji (wg kolejności z propozycji)

| # | Luka | Punkt |
|---|---|---|
| 15 | Kopiuj/wklej klucze | B |
| 16 | Zoom i przewijanie osi czasu, zaznaczanie ramką | C |
| 17 | Markery i obszar roboczy | D |
| 18 | Rodzic–dziecko warstw | E |
| 19 | Maski i matte | F |
| 20 | Tryby mieszania | G |
| 21 | Wiggle i proste ekspresje | H |
| 22 | Biblioteka presetów animacji (własne) | I |
| 23 | Local Stop (wiele punktów) | J |
| 24 | Kształty wektorowe | K |
| 25 | Skróty AE do odsłaniania właściwości, edytor skrótów, wybór języka w UI, paleta poleceń | L, M |
| 26 | Podgląd na żywo (hot reload) do OBS | N |
| 27 | Dostępność i skala UI | O |

Szczegóły, koszty i pochodzenie każdego punktu: [PROPOZYCJE-FUNKCJI.md](PROPOZYCJE-FUNKCJI.md).
