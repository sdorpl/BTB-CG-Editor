# Status prac

> Plik czytany na początku każdej sesji agenta. Aktualizuj go **w tym samym commicie**, który zmienia stan
> (zakończone zadanie, nowy bloker, zmiana kolejności). Historia szczegółowa: `git log`.
>
> **Ostatnia aktualizacja:** 2026-10-17

## Bieżący etap

Funkcje w stylu After Effects (klucze, krzywe, graph editor, ścieżka ruchu, efekty tekstowe) i silnik `btb-motion`
są gotowe (fazy 0–6, plan: [PLAN-btb-motion.md](PLAN-btb-motion.md)). Trwa etap **rozbudowy edytora o pracę z plikami**:
grafiki z dysku, paczka elementu `.cgpack`.

## Zrobione ostatnio

- G1/1: usunięto `unwrap()` na ścieżkach z danych zewnętrznych (render, app, timeline, keyconv, canvas/tools, motion_page, motion_bridge, compile, btb-motion/channel); zostały CLI w `main.rs`, `presets.rs`, `i18n.rs` (patrz LUKI.md).
- Dokumentacja aplikacji i instrukcja dla początkujących.
- Obrazy z pliku na dysku (wybór pliku, nowa warstwa, osadzanie) — punkt A z [PROPOZYCJE-FUNKCJI.md](PROPOZYCJE-FUNKCJI.md).
- Projekt formatu paczki elementu `.cgpack` ([FORMAT-PACZKI.md](FORMAT-PACZKI.md)) — sam projekt, bez implementacji.

## Następne zadanie

Wg kolejności z [PROPOZYCJE-FUNKCJI.md](PROPOZYCJE-FUNKCJI.md): **A** (dokończenie: przeciąganie z systemu, schowek,
przenoszenie pliku na serwer/do OBS; implementacja `.cgpack`) → **B** kopiuj/wklej klucze → **C** zoom osi czasu →
**J** Local Stop → **I** biblioteka presetów animacji.
Przed wyborem zadania przeczytaj [LUKI.md](LUKI.md): niezawodność (G1) idzie przed nowymi funkcjami.

## W toku / uruchomione agenty

(brak)

## Blokery i decyzje do podjęcia przez użytkownika

- Prawa do dystrybucji czcionek i grafik w przykładach (`assets/BAHNSCHRIFT.TTF`, `CG elements converted/`): do rozstrzygnięcia
  przed publicznym wydaniem (patrz `AGENTS.md` §2).
- Krok 5c (podmiana GSAP w `compile.rs`) wymaga zmian w BTB-CG (`cgmotion.js`, `renderer.js`); nie ruszamy bez zgody.
- Auto-szerokość elementów wymaga zmiany modelu sceny współdzielonego z BTB-CG.

## Jak pracować (skrót)

1. `git log -5`, `git status`, ten plik, `LUKI.md`.
2. Zadanie → odpowiedni dokument (`PLAN-btb-motion.md`, `PROPOZYCJE-FUNKCJI.md`) → referencja w `reference/effectcraft/`.
3. Zmiana → testy → `make check` → dokumentacja → commit (jedna spójna zmiana = jeden commit).
4. Zaktualizuj ten plik, jeśli zmienił się stan.
