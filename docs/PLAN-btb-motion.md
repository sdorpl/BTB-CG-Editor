# Plan: wspólny silnik animacji `btb-motion`

Cel: jeden deklaratywny silnik animacji (Director / Actor / Channel / Key / Event) używany
w edytorze (natywnie) i w `output.html` w OBS (WASM), zamiast dwóch implementacji (GSAP w JS
+ port w `src/anim.rs`). Zasada: **każdy krok jest weryfikowany automatycznie, nic nie jest
przełączane bez testu zgodności z obecnym zachowaniem**.

## Zasady
1. Obecny kod (`anim.rs`, `playback.rs`, `compile.rs`) jest wzorcem referencyjnym; nowy silnik
   ma dawać te same wartości (tolerancja zależna od kanału), a nie "podobne".
2. Silnik nie zależy od egui ani od modelu aplikacji. Most (`src/motion_bridge.rs`) jest
   po stronie aplikacji.
3. Każda faza kończy się komendą `make check` na zielono; błędy naprawiamy w tej samej fazie.

## Fazy
| # | Faza | Wynik | Weryfikacja |
|---|------|-------|-------------|
| 0 | Automatyzacja i workspace | root `[workspace]`, `Makefile`, `tools/check.sh` (fmt, clippy, testy, wasm) | `make check` |
| 1 | Rdzeń silnika: baking + zegar swobodny | `Handle::Hold`, `bake()` (adaptacyjne upraszczanie z zachowaniem skoków), `Director.free`, `Transport` z zegarem wstrzymania | testy jednostkowe + losowe (własny LCG, bez zależności) |
| 2 | Most `ElementAnimation -> Director` | `src/motion_bridge.rs`: IN/OUT elementu i warstw, pętle uwagi | pełna macierz typ x kierunek x ease x (in/out) vs `anim::eval_*` |
| 3 | Zgodność na poziomie odtwarzacza | `motion_bridge::evaluate` o sygnaturze `playback::evaluate`; przełącznik `BTB_ENGINE=motion` w aplikacji | test porównujący `evaluate` obu silników na presetach, w fazach In/Hold/Out |
| 4 | WASM | crate `btb-motion-wasm` (C ABI, bez wasm-bindgen), cel `wasm32-unknown-unknown` | `tools/wasm-parity.js` (Node): te same próbki co natywnie |
| 5 | Dalsze kroki (poza tym przebiegiem) | generowanie `output.html` z runtime WASM zamiast GSAP w `compile.rs`; efekty tekstowe jako kanały; kanał pozycji ze ścieżką/prędkością; Local Stop | opisane w sekcji "Otwarte" |

## Decyzje projektowe
- **Baking zamiast tłumaczenia easingów:** `anim.rs` ma easingi analityczne (elastic, bounce,
  back) i kroki (glitch), których nie da się wyrazić jednym segmentem Béziera. Silnik próbkuje
  funkcję i upraszcza ją do kluczy (Douglas-Peucker), a skoki zapisuje jako `Handle::Hold`.
  Błąd jest ograniczony parametrem `eps` i sprawdzany testami.
- **Pętle uwagi** (pulse, float…) działają w stanie Stop (hold), gdy oś czasu stoi, więc
  dostają zegar swobodny (`Director.free`), jak "Use System Time" w Viz.
- **Czas w sekundach (f32)**; Viz używa pól, ale wasza ścieżka jest niezależna od fps.

## Otwarte (po tym przebiegu)
- Efekty tekstowe per znak/słowo/linia (stagger) jako generator kluczy.
- `output.html`: runtime WASM + dane Director w JSON, API OBS (`obsstudio`).
- Local Stop z osobnym czasem per Director.
- Edytor osi czasu w egui (klucze, uchwyty).

## Status (wykonanie)
| Faza | Stan | Dowód |
|------|------|-------|
| 0 Automatyzacja | zrobione | `make check` = fmt + clippy `-D warnings` + `cargo test --workspace` + WASM/Node |
| 1 Rdzeń | zrobione | 12 testów `btb-motion/tests/model.rs` (bake, Hold, zegar swobodny, okna aktorów) |
| 2 Most | zrobione | `motion_bridge::tests`: macierz ~11 tys. próbek typ x kierunek x ease x in/out; pętle uwagi |
| 3 Odtwarzacz | zrobione, bez próby wizualnej | `player_matches_playback_on_presets` (6 typów elementów x 22 pary animacji x fazy In/Hold/Out); przełącznik `BTB_ENGINE=motion` w `app.rs` |
| 4 WASM | zrobione | `tools/wasm-parity.js`: dane z natywnego przebiegu i z prawdziwego elementu aplikacji (`app_lower_third`) zgodne co do 1e-4 |
| 5a Strona wyjściowa | zrobione | `src/motion_page.rs` + `assets/btb-motion-player.js` + CLI `--write-motion-html`; `tools/e2e-motion.js` (Chromium: TAKE, hold, OFF, aktualizacja pól, zrzuty) |
| 5b Zgodność z GSAP | zrobione | `tools/e2e-gsap-compare.js`: 32 warianty typ x kierunek + 6 elementów z warstwami i pętlami zgodne z `cgmotion.js` (poz. <=3 px, przezr. <=0,05, clip <=0,03, blur <=1,5 px) |
| 6 Efekty tekstowe | zrobione | `btb-motion/src/text.rs` (port `applyTextEffect`/`applyTextOutEffect`, stagger z `gsap.utils.distribute`), `TextTrack` w Directorze, dzielenie DOM w playerze; `E2E_SET=e2e-text`: 37 par IN x OUT zgodnych z GSAP per jednostka (znak/słowo) |
| 6b Easingi | zrobione | `tools/ease-parity.js`: 40 nazw zgodnych z `gsap.parseEase` (poprawione `sine.*`, dodane `quart`/`quint`) |
| 5c Podmiana GSAP w `compile.rs` | nie zrobione (celowo) | wymaga zmiany `cgmotion.js`/`renderer.js` w BTB-CG; test `compile_matches_btb_cg_scene_model` pilnuje kontraktu |

Znane ograniczenia: tolerancje wypieku (`BAKE_EPS` = 0,04% zakresu; ~1 px dla przesunięć, ~0,004 dla
opacity/skali) i położenie skoków z dokładnością do 1/240 s; test macierzy trwa ok. 75 s w debugu.

## Rozbieżności znalezione testem zgodności z GSAP (naprawione w `anim.rs`)
1. Punkt odniesienia elementu: na wizji domyślnie lewy górny róg (`.cg-scene { transform-origin: top left }`),
   port skalował wokół środka (`zoom`, `zoom-in`, `pop`, `skew`, pętle). Dotyczyło też podglądu w egui.
2. `wipe`, `wipe-y`: GSAP animuje przezroczystość razem ze skalą (port: skok).
3. `slide-overshoot`: przezroczystość podąża za easingiem `back` (port: `min(2*main, 1)`).
4. `shutter`, `mask-diag`, `rgb-split`: port tylko przybliżał; teraz `Transform` ma dokładne pola
   (`shutter`, `diag`, `diag_r`, `rgb`), a `Transform::approx()` daje przybliżenie dla egui.

## Jak używać
- `make check`: fmt, clippy, wszystkie testy, WASM, e2e w Chromium (jeśli jest Playwright w BTB-CG).
- Strona dla OBS: `btb-cgeditor --write-motion-html element.json output-motion.html`; w OBS: Browser Source
  z plikiem lokalnym, `?obs=1` (aktywne = TAKE, nieaktywne = reset), `?auto=1`, `?ws=ws://host:port`.
- Podgląd w edytorze silnikiem btb-motion: `BTB_ENGINE=motion btb-cgeditor`.

## Ograniczenia strony wyjściowej
Brak efektów tekstowych (stagger po znakach/słowach), brak Local Stop, nie sprawdzono w samym OBS (tylko
Chromium z Playwright). Layout (`side`, `margin`, `scale`, `rotation`) liczony z `Layout::frame_origin`,
bez porównania z `renderer.js`.

## Efekty tekstowe: co jest w silniku, co w przeglądarce
- Silnik (`btb-motion::text`): czas i właściwości każdej jednostki (`x, y, scale, opacity, blur, clip, progress`),
  stagger (`start/end/center/edges`, znormalizowany do `each*(n-1)`, limit `1,2/liczba`), czas całkowity.
- Przeglądarka (runtime): dzielenie DOM (`chars` = `Array.from`, `words` = `split(/(\s+)/)`, `lines` = słowa
  z liczbą linii z `offsetTop`), zliczanie jednostek (`set_units`), efekty treści (scramble, counter, kareta).
- Reguły odtworzone z cgmotion, bez których wyniki się rozjeżdżają:
  * `lines-*` animuje słowa (GSAP spłaszcza listę linii), a limit staggera liczy linie;
  * po pierwszym podziale element ma dzieci, więc efekt dzielący w innym trybie (albo po kursorze
    `typewriter-caret`) animuje cały element;
  * efekt własny warstwy wygrywa z efektem elementu także jako `none`;
  * OUT elementu typu `none` ustawia opacity 0 od razu (po opóźnieniu), a OUT elementu startuje dopiero po
    OUT warstw i ich tekstów (`base_out_start`).
- Ograniczenia: czas Stop jest liczony z tekstu domyślnego (po `update()` z innymi liczbami jednostek efekt
  może skończyć się po Stop; jednostki IN po Stop wracają do stanu końcowego); plan zakłada pełny podział
  tekstu na wyjściu, a po `typewriter-caret` GSAP animuje przez ok. 2 s cały element (scena znika wcześniej niż
  w planie); `scramble` używa losowych znaków, więc jego treści nie porównujemy; egui nadal rysuje efekty
  tekstowe przybliżeniami z `anim.rs` (bez blur i clip), ale z tymi samymi czasami i regułami wyboru efektu.

## Etap AE (wzorowany na EffectCraft)
| Element | Stan | Dowód |
|---|---|---|
| Model Ease (prędkość/wpływ), Easy Ease, auto-Bézier, continuous, `time_reverse` | zrobione | `btb-motion/tests/ease.rs` (formuła z oracle, losowe uchwyty, odwrócenie czasu) |
| Edycja kluczy w silniku (`insert/remove/move_keys`), matematyka uchwytów (`handle_point`/`set_handle_point`) | zrobione | `tests/edit.rs`, `tests/ease.rs` |
| Klucze użytkownika (`Layer.keys`), `stopAt`, składanie źródeł | zrobione | `motion_bridge::tests`, `tools/e2e-motion.js` (`keys_pure`) |
| Oś czasu z kluczami | zrobione | `panels::timeline::tests` (symulowane zdarzenia: klik, przeciąganie, dwuklik, Delete, Shift) |
| Edytor krzywych | zrobione | `panels::graph::tests` (uchwyty, Shift, klucze, widok prędkości, kanał ścieżki) |
| Ścieżka ruchu | zrobione | `btb-motion/tests/path.rs`, `canvas::tests`, `tools/e2e-motion.js` (`path_pure`) |
| Poprawka błędu: stan początkowy OUT w hold (glitch, `none`) | zrobione | test `HOLD` w `tools/e2e-gsap-compare.js` |

Adaptacje względem referencji: tangent auto-Béziera ograniczony do połowy odległości do sąsiada (bez pętli przy nierównych
odstępach); odwrócenie czasu negujące prędkości; ruch po łuku modelowany kanałem odległości (cały model uchwytów czasowych
działa bez zmian). Kod EffectCraft nie był kopiowany hurtem (MIT OR Apache-2.0, koncepcje i matematyka).

## Konwersja presetów na klucze i import (etap po AE)
| Element | Stan | Dowód |
|---|---|---|
| `Channel::fit`: funkcja czasu -> kilka kluczy Béziera (podział w najlepszym punkcie, `Hold` na skokach) | zrobione | `btb-motion/tests/fit.rs`: `power2.out` = 2 klucze, `back.out` = 2, `elastic.out` ~10, `bounce.out` ~9 |
| `keyconv::convert_presets` (element, warstwy, grupy), klucze elementu w `Scene.keys` | zrobione | `keyconv::tests` (160 wariantów typ x kierunek x easing równoważnych co do wartości) |
| Zgodność skonwertowanych elementów z GSAP oryginału (klucze w WASM, Chromium) | zrobione | `e2e-gsap-compare.js` używa `<nazwa>.converted.json` dla elementów `e2e`, `e2e-matrix` |
| Import: intencja animacji (domyślne efekty tekstu, `onShow`), uwagi, rekompilacja | zrobione | `convert::tests::real_db_templates_keep_their_animation_intent`; zgodność efektów tekstu 29/34 -> 34/34 |
| Import: sonda auto-szerokości (ostrzeżenie + ściskanie tekstu) | zrobione | wykrywa 8 z 34 elementów, dokładnie te, które rozjeżdżają się po zmianie tytułu |
| Import: auto-szerokość w modelu sceny | nie zrobione | wymaga zmiany `scene-model.js` w BTB-CG i `compile.rs` jednocześnie |
