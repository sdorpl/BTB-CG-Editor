# btb-cgeditor

Edytor grafiki telewizyjnej (CG) w Rust + egui. Tworzy elementy (lower thirds itp.) ze sceną, warstwami i animacjami
IN / OUT / pętle uwagi, a eksport trafia na stronę `output.html` używaną w OBS (Browser Source).
Aktualny kierunek prac: **funkcje i animacje wzorowane na After Effects** (klucze, krzywe, graph editor, motion path, efekty tekstowe).

Język pracy z użytkownikiem i dokumentacji: **polski**. Identyfikatory i komentarze w kodzie zgodnie z otoczeniem
(kod jest po angielsku, dokumenty po polsku).

## Zacznij każdą sesję tutaj

1. Przeczytaj `docs/STATUS.md` (bieżący etap, następne zadanie, blokery) oraz `git log -5` i `git status`.
2. Przeczytaj zadanie w `docs/PROPOZYCJE-FUNKCJI.md` (lista i kolejność), a przy silniku `docs/PLAN-btb-motion.md`; przy zapisie/eksporcie `docs/FORMAT-PACZKI.md`.
3. Przeczytaj `docs/LUKI.md`: każdy znany brak, uszeregowany. Niezawodność i wierność wyjścia (G1–G2) idą przed nowymi funkcjami.
4. Przy funkcjach w stylu After Effects: odpowiedni moduł w `reference/effectcraft/` (patrz „Repozytorium referencyjne”).
5. Pracuj samodzielnie wg „Styl pracy” i „Commity”; pytaj tylko, gdy decyzja należy do użytkownika. Po zmianie stanu zaktualizuj `docs/STATUS.md` w tym samym commicie.

## Niepodważalne zasady

**Najpierw przeczytaj [`AGENTS.md`](AGENTS.md); jego reguły mają pierwszeństwo nad tym plikiem** (nie wywalaj aplikacji, zasoby tylko na otwartych licencjach, clean-room).

- **Nie wywalaj aplikacji:** nowy kod bez `unwrap()`/`expect()`/`panic!`, błędy przez `Result` i `?`, dane z pliku/serwera/importu traktuj jako wrogie (`get()`, NaN/inf, limity rozmiaru). Każda poprawka awarii ma test regresji.
- **Parytet ze wzorcem:** `anim.rs`, `playback.rs`, `compile.rs` to wzorzec; `btb-motion` ma dawać te same wartości, nie „podobne” (zasady silnika niżej).
- **Warstwy:** `btb-motion*` nie zależy od egui ani od modelu aplikacji; wszystko, co zna model, idzie do mostu w `src/`. Silnik buduje się także dla `wasm32-unknown-unknown`.
- **Czas** w sekundach (`f32`), niezależnie od fps.
- **Nie psuj edytora:** kod nieprzetestowany nie trafia do przełączników; nowe testy losowe na własnym LCG, bez nowych zależności.
- **Teksty UI** tylko przez `i18n::tr`/`trf`, klucze w **obu** plikach `pl.json` i `en.json` w tym samym commicie.
- **Dokumentacja** (`docs/DOKUMENTACJA.md`, `docs/INSTRUKCJA-poczatkujacy.md`) aktualizowana w commicie zmieniającym zachowanie aplikacji.
- **Referencja EffectCraft:** adaptuj, nie kopiuj hurtem (MIT/Apache-2.0, przy większych fragmentach dopisz pochodzenie). Nie commituj `reference/`.
- **Bramka jakości przed każdym commitem:** `make check` (fmt `btb-motion*`, clippy `-D warnings`, testy, WASM + parity, e2e). Nie uruchamiaj `make fmt` na całym `src/`.
- **Commity:** jedna spójna zmiana = jeden commit, po polsku, tylko zielone stany, pliki dodawane jawnie. `git push` tylko na prośbę.
- **Nie zmieniaj** `target-dir` w `.cargo/config.toml` (Nextcloud blokuje binarki build-script).

## Uruchamianie i oglądanie aplikacji

- Przy pracy nad UI **zobacz wynik**: `btb-cgeditor --open plik.json --shot out.png --frames 25 [--select N] [--graph] [--phase hold:0.3]` (okno otwiera się na chwilę), lub `make ui-shots` (scenariusze `tests/ui/scenarios.json`, wzorce `tests/ui/golden/`; wymaga wyświetlacza i Pillow).
- Strona dla OBS: `btb-cgeditor --write-motion-html element.json output-motion.html`; zgodność z GSAP: `E2E_SET=e2e|e2e-matrix|e2e-text node tools/e2e-gsap-compare.js`.
- Podgląd silnikiem btb-motion: `BTB_ENGINE=motion btb-cgeditor`; GPU: `--gpu auto|on|off`, `--renderer glow|wgpu`; język: `BTB_LANG=en`.
- Zrzuty (`*.png`) są tymczasowe: trzymaj je poza gitem (np. w katalogu scratchpad).
- Pełna lista komend: sekcja „Komendy” niżej.

## Struktura

| Ścieżka | Rola |
|---|---|
| `src/` | aplikacja (egui): `app.rs`, `canvas.rs` (z nakładką ścieżki ruchu), `model.rs`, `render.rs`, `compile.rs`, `importer.rs`, `panels/` (inspector, layers, `timeline.rs` = oś czasu z kluczami, `graph.rs` = edytor krzywych) |
| `src/i18n.rs`, `assets/lang/{pl,en}.json` | tłumaczenia UI (patrz „Tłumaczenia”) |
| `src/anim.rs`, `src/playback.rs` | **wzorzec referencyjny** animacji (port GSAP z BTB-CG) |
| `src/motion_bridge.rs` | most `ElementAnimation -> Director` (po stronie aplikacji) |
| `src/keyconv.rs` | konwersja presetów IN/OUT na klucze (`convert_presets`), `Scope::{All, Element, Layer}` |
| `src/convert.rs`, `tools/convert-bridge.js` | import ręcznych szablonów BTB-CG: wywołuje `SceneConverter` z BTB-CG w Chromium i odtwarza intencję animacji (patrz niżej) |
| `src/motion_page.rs`, `assets/btb-motion-player.js` | generowanie strony wyjściowej z runtime WASM |
| `btb-motion/` | silnik animacji bez zależności od egui i modelu aplikacji: Director / Actor / Channel / Key / Event, `ease.rs` (easingi GSAP), `text.rs` (efekty tekstowe), `path.rs` (ścieżka ruchu), model Ease (prędkość/wpływ) w `curve.rs` |
| `btb-motion-wasm/` | ten sam silnik jako WASM (C ABI, bez wasm-bindgen), cel `wasm32-unknown-unknown` |
| `tools/` | `check.sh`, `build-wasm.sh`, testy Node/Playwright (parity WASM, e2e, zgodność z GSAP), `convert-quality.js` (jakość importu) |
| `docs/DOKUMENTACJA.md`, `docs/INSTRUKCJA-poczatkujacy.md` | dokumentacja aplikacji i instrukcja dla początkujących, **aktualizowane przy każdym commicie zmieniającym aplikację** |
| `AGENTS.md`, `docs/STATUS.md`, `docs/LUKI.md`, `docs/PROPOZYCJE-FUNKCJI.md` | reguły nienaruszalne; bieżący stan i następne zadanie; ranking znanych braków; lista funkcji do wdrożenia |
| `docs/PLAN-btb-motion.md` | plan, decyzje projektowe i status faz silnika, **czytaj przed zmianami w silniku** |
| `reference/effectcraft/` | repozytorium referencyjne (patrz niżej), poza gitem |

## Komendy

- `make check` – pełna weryfikacja: fmt, clippy `-D warnings` (btb-motion, btb-motion-wasm), `cargo test --workspace`, WASM + parity w Node, e2e w Chromium (jeśli jest Playwright w BTB-CG; katalog z `BTB_CG_DIR`, domyślnie `~/Nextcloud/GitHub/BTB-CG-System-alpha`).
- `make fmt`, `make lint`, `make test`, `make wasm`, `make parity`, `make e2e`. Zestawy porównania z GSAP: `E2E_SET=e2e|e2e-matrix|e2e-text node tools/e2e-gsap-compare.js` (`VERBOSE=1`, `RAW=1`, `ONLY=regex` do diagnozy); easingi: `node tools/ease-parity.js`.
- Zrzut edytora do oglądania: `btb-cgeditor --open plik.json --shot out.png --frames 25 [--select N] [--graph] [--phase hold:0.3]` (okno otwiera się na chwilę).
- Strona dla OBS: `btb-cgeditor --write-motion-html element.json output-motion.html`; w OBS parametry `?obs=1`, `?auto=1`, `?ws=ws://host:port`.
- Akceleracja sprzętowa okna (`src/gpu.rs`): `--gpu auto|on|off` (`BTB_GPU`; auto = GPU z awaryjnym programowym, on = wymagane, off = programowo) i `--renderer glow|wgpu` (`BTB_RENDERER`; domyślnie glow/OpenGL, wgpu = Vulkan/Metal/DX12). CLI ma pierwszeństwo przed zmienną. Tryb `--gpu` działa dla glow; wgpu zawsze wybiera adapter o wysokiej wydajności.
- Testy wizualne UI: `make ui-shots` (`tools/ui-shots.py`): scenariusze z `tests/ui/scenarios.json` na `tests/ui/fixture.json`, porównanie ze wzorcami `tests/ui/golden/*.png` (`--update` odświeża). Wymaga wyświetlacza i Pillow, nie wchodzi do `make check`; wzorce zależą od maszyny/czcionek.
- Podgląd silnikiem btb-motion w edytorze: `BTB_ENGINE=motion btb-cgeditor`.
- `target-dir` jest ustawiony w `.cargo/config.toml` na `/tmp/btb-cgeditor-target` (Nextcloud blokuje uruchamianie binarek build-script w synchronizowanym drzewie). Nie zmieniaj tego.
- Testy macierzy zgodności trwają ok. 75 s w debugu; `btb-motion` jest kompilowany z `opt-level = 3` także w dev/test.

## Zasady silnika `btb-motion` (z `docs/PLAN-btb-motion.md`)

1. `anim.rs`, `playback.rs`, `compile.rs` to wzorzec: nowy silnik ma dawać **te same wartości** (tolerancje zależne od kanału), nie „podobne”.
2. Silnik nie zależy od egui ani od modelu aplikacji. Wszystko, co zna model, idzie do mostu w `src/`.
3. Każda faza/zmiana kończy się `make check` na zielono; błędy naprawiamy w tej samej zmianie.
4. Nic nie jest przełączane bez testu zgodności z obecnym zachowaniem. Nowe testy losowe: własny LCG, bez nowych zależności.
5. Czas w sekundach (`f32`), niezależnie od fps.
6. Baking zamiast tłumaczenia easingów: easingi analityczne (elastic, bounce, back) i kroki (glitch) są próbkowane i upraszczane (Douglas-Peucker, `BAKE_EPS` = 0,04% zakresu), skoki jako `Handle::Hold` (dokładność 1/240 s).
7. Pętle uwagi (pulse, float…) działają w stanie Stop na zegarze swobodnym (`Director.free`), jak „Use System Time” w Viz.
8. Domyślny punkt odniesienia elementu na wizji to lewy górny róg (`.cg-scene { transform-origin: top left }`), nie środek.

Stan faz: 0–6 zrobione (rdzeń, most, odtwarzacz, WASM, strona wyjściowa, zgodność z GSAP, efekty tekstowe, easingi). 5c (podmiana GSAP w `compile.rs`) celowo niezrobione: wymaga zmian w `cgmotion.js`/`renderer.js` w BTB-CG, kontrakt pilnuje test `compile_matches_btb_cg_scene_model`.

## Konwersje: presety na klucze, import ręcznych szablonów

- **Presety → klucze** (`keyconv::convert_presets`, przycisk „Presety na klucze” na osi czasu i menu elementu/warstwy): krzywe są dopasowywane kilkoma kluczami z uchwytami Béziera (`Channel::fit`, tolerancja min(0,15% zakresu, 0,6 px / 0,15° / 0,4%)), IN i OUT sklejane w jeden kanał z `Hold` na przeskokach stanu; klucze elementu leżą w `Scene.keys` (cel `"element"`). Po konwersji preset = `none`, a Stop zostaje (`stopAt`); OUT elementu `none` nie chowa elementu, gdy ma on klucze. Pętle i efekty tekstowe zostają presetami. Test zgodności: `keyconv::tests` (macierz typów) oraz porównanie strony z kluczami z GSAP oryginału (`<nazwa>.converted.json` w zestawach `e2e*`).
- **Klucze OUT zaczynają się o `OUT_EPS` po `t_in`**, tak jak okna aktorów (inaczej w hold widać stan początkowy OUT).
- **Import ręcznych szablonów**: konwerter DOM z BTB-CG czyta tylko stan końcowy. Most (`convert-bridge.js`, `BTBIntent`) podmienia `CGMotion.mount`/`build`, renderuje oryginał i odtwarza: domyślne efekty tekstu zapisane w kodzie szablonu, dodatkowe animacje z `onShow` (przypisywane warstwie po klasie elementu, potem po pokryciu prostokątów) oraz ostrzeżenie o auto-szerokości (sonda: render z krótkim i długim tekstem). Notatki trafiają do „Uwag konwersji”, a element jest rekompilowany po stronie Rust.
- **Jakość importu**: `make convert-quality` (`tools/convert-quality.js`): różnica pikseli w stanie końcowym (zegar zamrożony), zgodność intencji animacji (efekty tekstu przed/po), opcjonalnie `TITLE=…` (konwersja przy innym tytule) i `DYN=…` (inne dane po konwersji). Pomiar animacji w czasie (`ANIM=1`) jest **nieważny** dla starych elementów: renderer ma własną animację CSS (np. `scale(0.85)` dla `zoom`), której przewijanie osi GSAP nie odtwarza.

## Klucze użytkownika, ścieżka ruchu (poza GSAP)

- Model: `Layer.keys: Vec<btb_motion::Channel>` (kanały `tx ty sx sy rot opacity`, wartości absolutne), `Layer.path: Option<PathChannel>` (pozycja po krzywej Béziera, przesunięcie od środka warstwy) i `animation.stopAt` (ręczna pozycja Stop). Czas kluczy = czas Directora: do Stop to IN, po Stop (`plan.in_total`) to OUT.
- Składanie źródeł (preset IN/OUT, klucze, ścieżka) na tej samej właściwości: `tx ty rot skew blur` sumują się, `sx sy opacity fx fy sh` mnożą się, reszta = ostatnie źródło. Ta sama reguła jest w `motion_bridge::to_transforms` i w `assets/btb-motion-player.js`; zmieniaj obie naraz.
- Okna aktorów OUT zaczynają się po `t_in + OUT_EPS` (`motion_bridge`): w stanie Stop transport stoi dokładnie w `t_in`, więc stan początkowy OUT (glitch, `none`) nie może wtedy działać. Test `HOLD` w `e2e-gsap-compare.js` tego pilnuje.
- Podgląd w edytorze używa silnika automatycznie, gdy element ma klucze lub ścieżkę (`motion_bridge::uses_keys`); inaczej domyślnie `playback::evaluate`.
- Klucze i ścieżka nie mają odpowiednika w GSAP/`compile.rs`: działają tylko na stronie wyjściowej z WASM (`--write-motion-html`). `tools/e2e-gsap-compare.js` pomija elementy `keys_*` i `path_*`.
- Testy interakcji UI używają symulowanych zdarzeń egui (`egui::Context::run` + `RawInput`). Uwaga: **nie dodawaj `on_hover_text` na widżetach, które mają być klikane w tych testach** (tooltip przechwytuje naciśnięcie); etykiety rysujemy `painter.text`.
- Model Ease z EffectCraft: `Handle::Ease { speed, influence }`, `Handle::Auto` (auto-Bézier 1/6), `Channel::{side_ease, set_ease, easy_ease, set_handle_point, velocity, time_reverse}`. Odwrócenie czasu zmienia znak prędkości (inaczej niż w kodzie referencyjnym; test na losowych uchwytach to potwierdza).

## Repozytorium referencyjne: EffectCraft

`reference/effectcraft/` to płytki klon https://github.com/storytold/effectcraft (open-source klon After Effects w Rust/egui, licencja MIT OR Apache-2.0, bez katalogu `.git`). Folder jest w `.gitignore` i w `exclude` workspace'u Cargo; **nie jest zależnością i nie jest budowany**. Aby odświeżyć: usuń folder i sklonuj ponownie.

Służy do czerpania wzorców dla funkcji w stylu After Effects. Najważniejsze miejsca:

- `crates/keyframe/src/lib.rs`: `Keyframe`, `Interp` (Linear/Bezier/Hold), `Ease` (prędkość + wpływ/influence po obu stronach klucza), `ease_progress`, `easy_ease`, `spatial_tangents` / `spatial_handles`, `retime_roving`, `velocity`, `speed`, `time_reverse`.
- `crates/ui-egui/src/panels/graph.rs`, `graph_tools.rs`: graph editor (krzywe wartości i prędkości, uchwyty, Shift blokuje oś, dopasowanie widoku).
- `crates/ui-egui/src/panels/timeline.rs`: oś czasu (warstwy, klucze, zaznaczanie).
- `crates/expr`, `crates/text`: ekspresje i tekst (na później).
- `docs/` (m.in. `ui-parity.md`, `architecture.md`): opis zakresu i architektury.

Zasady korzystania:
- Przed implementacją funkcji w stylu AE przeczytaj odpowiedni moduł referencyjny.
- **Adaptuj, nie kopiuj hurtem.** Ich model jest ogólny (`Tick`, `Value`, wiele wymiarów); nasz jest prostszy (`f32`, sekundy, kanał = jedna własność). Przenoś koncepcje i matematykę, zachowaj licencyjną uczciwość (MIT/Apache-2.0: przy przenoszeniu większych fragmentów dodaj informację o pochodzeniu).
- Część silnikowa trafia do `btb-motion` (bez egui, z testami), część UI do `src/panels/`.

## Plan prac (ustalony w sesji)

Kolejność dla funkcji w stylu After Effects, wszystkie cztery punkty zrobione:

1. ~~Ease prędkość/wpływ w `btb-motion`~~: `curve.rs` (`Ease`, `Handle::Ease/Auto`), `channel.rs`, testy `btb-motion/tests/ease.rs`.
2. ~~Oś czasu z kluczami~~ (`src/panels/timeline.rs`): wiersze kanałów i ścieżki, zaznaczanie, przeciąganie (z migawki), dwuklik = nowy klucz, Delete, menu interpolacji, pasek szczegółów (Stop, czas, wartość, prędkość/wpływ).
3. ~~Graph editor~~ (`src/panels/graph.rs`): wartość i prędkość, uchwyty Béziera (poziomo wpływ, pionowo prędkość, Shift blokuje czas), przeciąganie kluczy.
4. ~~Kanał pozycji ze ścieżką~~ (`btb-motion/src/path.rs`, nakładka w `canvas.rs`): tangenty auto/ręczne, ruch po długości łuku, roving, edycja na kanwie.

Dalej (z planu): Local Stop z osobnym czasem per Director; ekspresje; ewentualnie podmiana GSAP (5c); rubber-band i zoom osi czasu; kopiuj/wklej klucze; konwersja presetów IN/OUT na klucze (żeby je edytować w edytorze krzywych).

## Znane ograniczenia

- Strona wyjściowa: brak Local Stop; efekty tekstowe są (zgodne z GSAP), ale czas Stop liczony z tekstu domyślnego; sprawdzona tylko w Chromium (Playwright), nie w samym OBS.
- Podgląd egui rysuje efekty tekstowe i maski (żaluzja, ukośna maska, RGB) przybliżeniami (`Transform::approx`, `anim.rs`); dokładny wygląd daje strona wyjściowa.
- Oś czasu: brak zoomu i zaznaczania ramką; ręcznie dodawane klucze to `KEY_PROPS` (6 właściwości) i ścieżka, ale konwersja presetów tworzy też kanały pomocnicze (skos, rozmycie, przycięcie, maska…).
- Import: elementy o **auto-szerokości** (belki „NA ŻYWO”, „MIASTO”…) po konwersji mają stały rozmiar tła; po zmianie tytułu rozjeżdżają się z oryginałem (57–84% pikseli przy `DYN=…`). Wykrywa to sonda (ostrzeżenie + ściskanie tekstu); prawdziwa naprawa wymaga auto-szerokości w modelu sceny, który jest współdzielony z BTB-CG (`scene-model.js`) i pilnowany testem `compile_matches_btb_cg_scene_model`.
- Layout (`side`, `margin`, `scale`, `rotation`) liczony z `Layout::frame_origin`, bez porównania z `renderer.js`.
- Folder projektu leży w Nextcloud: `reference/effectcraft/` (ok. 1100 plików) też będzie synchronizowany; jeśli to problem, dodaj go do wyłączeń synchronizacji.

## Styl pracy

- Pisz kod zgodny z otoczeniem (gęstość komentarzy, nazewnictwo, idiomy). `make check` sprawdza formatowanie tylko w `btb-motion*`; `src/` ma dłuższe linie, więc nie uruchamiaj `make fmt` (`cargo fmt --all`) na całości bez zgody.
- Nie rozszerzaj zakresu poza prośbę; po zmianie uruchom `make check` (lub jego odpowiedni fragment) i podaj wynik zgodnie z prawdą.

## Dokumentacja (aktualizacja przy każdym commicie)

- Dokumentacja użytkowa: `docs/DOKUMENTACJA.md` (opis aplikacji i funkcji) oraz `docs/INSTRUKCJA-poczatkujacy.md` (instrukcja dla początkujących). Po polsku.
- **Każdy commit zmieniający zachowanie aplikacji** (UI, skróty, menu, model/format pliku, animacje, eksport, CLI, zmienne środowiskowe, ograniczenia) **musi w tym samym commicie zaktualizować dokumentację**: właściwą sekcję `DOKUMENTACJA.md`, a gdy zmiana dotyka codziennej pracy – także `INSTRUKCJA-poczatkujacy.md` (kroki, skróty, rozwiązywanie problemów).
- Zmiana wyłącznie wewnętrzna (refaktor, testy) nie wymaga edycji dokumentacji; w razie wątpliwości sprawdź, czy opis w docs nadal jest prawdziwy.
- Zaktualizuj też tabelę „Struktura” i „Znane ograniczenia” w tym pliku, gdy się zmieniły. Dokumentacja i kod nie mogą się rozjechać: usuń opis funkcji, którą usunięto.
- Nowa funkcja z napisami UI: dokumentacja używa polskich etykiet z `assets/lang/pl.json`.

## Commity (agent w chmurze)

- Po **każdej** zmianie w kodzie (i dokumentacji) zakończonej na zielono zrób commit, zanim przejdziesz do następnej zmiany. Jedna spójna zmiana = jeden commit; nie zostawiaj niezacommitowanej pracy na koniec sesji.
- Przed commitem uruchom `make check` (lub jego odpowiedni fragment, np. `cargo test -p btb-motion`) i napraw błędy w tej samej zmianie. Nie commituj czerwonych testów; jeśli czegoś nie dało się zweryfikować (np. brak Playwright), napisz to w treści commita.
- Dodawaj pliki jawnie (`git add ścieżka…`), nie `git add -A`/`git add .`. Nie commituj: `reference/`, `vendor/` (jeśli nie jest częścią zmiany), plików tymczasowych, artefaktów builda, zrzutów (`*.png` z `--shot`) ani cudzych, niezwiązanych zmian z drzewa roboczego. Sprawdź `git status` i `git diff --staged` przed `git commit`.
- Nie używaj `--no-verify`, `--amend` na już istniejących commitach ani `git push --force`. Nie przepisuj historii. `git push` tylko na wyraźną prośbę użytkownika.
- Komunikat: pierwsza linia w trybie rozkazującym, do ok. 72 znaków, po polsku (np. „Dodaj zoom osi czasu”), z opcjonalnym opisem „dlaczego” po pustej linii. Na końcu wiersz atrybucji wymagany przez środowisko (`Co-Authored-By: …`).
- Zmiany w jednej warstwie trzymaj razem, ale oddzielaj commity: silnik (`btb-motion*`), UI (`src/`), tłumaczenia/dokumentacja. Zmiany w `motion_bridge::to_transforms` i `assets/btb-motion-player.js` idą w jednym commicie (patrz „Klucze użytkownika”).
- Nowy klucz tłumaczenia dodany do `pl.json` i `en.json` idzie w tym samym commicie co kod, który go używa.

## Ściskanie tekstu (squash)
- Zasada jak w `renderer.js` `applyGlobalSquashing`: domyślnie dopasowanie do szerokości pudełka; BADGE (`data-squash-mode=length`) → skala `max(0.5, 25/długość)` dla >25 znaków, kotwica z lewej. Zaimplementowane w `render::squash_factor` (edytor, `SceneParams.badge_squash`) i `P.squash` w `assets/btb-motion-player.js` (`cfg.squash`).
- Konwerter (`convert-bridge.js`, blok `squashed`) dla BADGE zachowuje pełną szerokość układu warstwy i lewą krawędź oryginału; elementy skonwertowane przed tą zmianą wymagają ponownej konwersji.
- `convert-quality` renderuje kod bez sceny (`renderForMeasure` usuwa `scene`), więc przy tekstach ściskanych pokazuje podwójne ściskanie — to artefakt narzędzia, nie runtime.

## Tłumaczenia (PL / EN)
- Wszystkie teksty UI idą przez `i18n::tr("klucz")` / `trf("klucz", &[("arg", &wartość)])` (`{arg}` w tekście); etykiety list rozwijanych przez `panels::combo_t` (`i18n::opt("opt.anim", wartość)`), wartość zapisana w pliku zostaje techniczna. Pliki: `assets/lang/pl.json`, `assets/lang/en.json` (płaskie mapy, wkompilowane `include_str!`). Brak klucza → polski → sam klucz.
- Nowy napis: dodaj klucz do **obu** plików. Testy `i18n::tests` pilnują: te same klucze w obu językach, te same `{argumenty}`, każdy literał `tr("…")` z kodu istnieje, listy opcji (`ANIM_TYPES`, `TEXT_IN`…) mają etykiety.
- Wybór języka nie ma jeszcze UI: `i18n::set_lang`, tymczasowo zmienna `BTB_LANG=en`. Domyślnie polski.
- Nie tłumaczone: komunikaty z `tools/convert-bridge.js` (notatki konwersji, po polsku), nazwy easingów GSAP, logi testów, komentarze w kodzie.
