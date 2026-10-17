# BTB CGEditor – dokumentacja aplikacji

Dokument opisuje, czym jest aplikacja, co potrafi i jak jest zbudowana. Dla osób zaczynających pracę: [INSTRUKCJA-poczatkujacy.md](INSTRUKCJA-poczatkujacy.md).
Plany i decyzje silnika animacji: [PLAN-btb-motion.md](PLAN-btb-motion.md). Analizy: [ANALIZA-edytory.md](ANALIZA-edytory.md), [RAPORT-panel-warstwy.md](RAPORT-panel-warstwy.md).

> Dokumentacja jest aktualizowana przy każdym commicie zmieniającym aplikację (zasada w `CLAUDE.md`).

## 1. Czym jest BTB CGEditor

Edytor grafiki telewizyjnej (CG – character generator) napisany w Rust z użyciem egui. Służy do tworzenia **elementów**
na wizję (belki z imieniem i funkcją – *lower thirds*, plakietki, tickery, zegary, logo, dowolne grafiki), które
składają się ze **sceny** (pole 1920×1080, w nim ramka elementu), **warstw** i **animacji** (wejście IN, wyjście OUT,
pętle uwagi, efekty tekstu).

Edytor jest częścią ekosystemu **BTB-CG**:

- elementy zapisywane są w formacie JSON zgodnym z BTB-CG (`scene-model.js`, `element-model.js`); nieznane pola są
  zachowywane (import → edycja → eksport bez strat),
- gotowy element można wysłać do banku lub biblioteki serwera BTB-CG albo wyeksportować jako samodzielną stronę
  `output.html` do **OBS** (Browser Source),
- istniejące elementy można zaimportować z serwera BTB-CG, także te pisane ręcznie kodem (konwersja na scenę).

Kierunek rozwoju: funkcje i animacje wzorowane na **After Effects** – klucze animacji, krzywe, edytor krzywych
(graph editor), ścieżka ruchu, efekty tekstowe.

## 2. Model danych

| Pojęcie | Opis |
|---|---|
| **Element** | Jednostka zapisywana w pliku: nazwa, id, typ, scena, layout (pozycja na wizji), animacja elementu, wartości pól, kod HTML/CSS/JS. |
| **Typ elementu** | `LOWER_THIRD` (belka), `BADGE` (plakietka/tag), `TICKER`, `CLOCK`, `IMAGE` (obraz/logo), `GRAPHIC` (grafika swobodna). Typ wpływa na szkic domyślny i zachowanie w panelu operatora BTB-CG. |
| **Scena** | Ramka (rozmiar elementu), lista warstw, klucze elementu (`Scene.keys`). |
| **Warstwa** | Typ: kształt, tekst, obraz, wideo, zegar, ticker. Ma pozycję, rozmiar, obrót, skos, krycie, grupę, właściwości typu, ruch (presety IN/OUT, efekty tekstu), klucze (`Layer.keys`) i opcjonalną ścieżkę ruchu (`Layer.path`). |
| **Layout** | Położenie ramki elementu na wizji: kotwica (lewy górny, prawy dolny, środek… lub własna pozycja), marginesy, skala, obrót, przezroczystość. Punktem odniesienia jest lewy górny róg. |
| **Pole (field/bind)** | Nazwa wartości podstawianej na antenie (np. `TITLE`, `LOGO_URL`). Warstwa tekstu lub obrazu może być powiązana z polem; panel operatora BTB-CG edytuje te pola. |
| **Klucz / kanał** | Kanał to jedna właściwość warstwy w czasie (`tx ty sx sy rot opacity`, także kanały pomocnicze po konwersji presetów); klucz to wartość w chwili czasu z uchwytami krzywej. |
| **Stop** | Moment zatrzymania na wizji. Klucze przed Stop należą do wejścia (IN), po Stop – do wyjścia (OUT). Domyślnie wyliczany z animacji, można ustawić ręcznie (`stopAt`). |

Czas liczony jest w sekundach, niezależnie od liczby klatek na sekundę.

### Elementy kodowe a sceny

Element może mieć scenę (edytowalny graficznie) albo tylko **kod ręczny** (HTML/CSS/JS napisany przez człowieka).
Dla kodu ręcznego edytor pokazuje kod, podgląd HTML i przycisk „Utwórz scenę”. Importer potrafi taki kod
**skonwertować na scenę** (patrz sekcja 7).

## 3. Interfejs

Okno (domyślnie 1600×900) składa się z:

1. **Paska menu**: Plik, Edycja, Widok + sterowanie odtwarzaniem (▶ IN, ▶ OUT, ⟳ Cykl, ■ Stop).
2. **Panelu warstw** (lewo): przyciski dodawania warstw (kształt, tekst, obraz, wideo, zegar, ticker), pozycja „▣ Element (całość)”, lista warstw z widocznością (👁), blokadą (🔒), kolejnością (wyżej/niżej), duplikowaniem i usuwaniem.
3. **Kanwy** (środek): pole 1920×1080 z ramką elementu, uchwytami, prowadnicami i nakładką ścieżki ruchu. Pasek: dopasowanie widoku (⛶ Pole, ▭ Ramka), 🌐 Podgląd HTML, obszary bezpieczne, przyciąganie.
4. **Inspektora** (prawo), cztery zakładki:
   - **Właściwości** – dla zaznaczonej warstwy: przekształcenie (położenie, rozmiar, obrót, skos, krycie, grupa), wygląd i właściwości typu (wypełnienie, gradient, obrys, zaokrąglenia, cień, efekt tła; czcionka i tekst; obraz/wideo; zegar; ticker), wyrównanie, sekcja zaawansowana. Przy zaznaczeniu wielu warstw: wyrównanie, rozmieszczenie i przesunięcie zbiorcze. Przy zaznaczeniu całego elementu panel odsyła do zakładki Element.
   - **Element** – nazwa, ID, typ, ramka sceny, layout na wizji (kotwica, marginesy, skala, obrót, przezroczystość), wartości pól (TITLE itd.), pola wejściowe (`ocgInputs`), przyciski tworzenia sceny / przywracania kodu.
   - **Ruch** – przełącznik zakresu **Element / Warstwa**. Dla elementu: IN, OUT, efekty tekstu, synchronizacja tekstu, pętla uwagi. Dla warstwy: IN, OUT, efekt tekstu IN/OUT, „Po warstwie” (start po zakończeniu fazy innej warstwy), stagger, kopiowanie ruchu do innych warstw; informacja o kluczach i ścieżce warstwy oraz przycisk „Usuń klucze”.
   - **Kod** – HTML/CSS/JS elementu (edytowalny dla kodu ręcznego, tylko do odczytu dla scen).
5. **Osi czasu** (dół): wiersze elementu i warstw z kanałami kluczy, ścieżką ruchu, polem Stop i paskiem szczegółów klucza.
6. **Edytora krzywych** (okno pływające, przełącznik „Wykres” na osi czasu).
7. **Paska statusu**: komunikaty, liczba warstw i zaznaczonych, zoom.

Interfejs jest dwujęzyczny (PL/EN) – patrz sekcja 9.

## 4. Edycja na kanwie

| Działanie | Efekt |
|---|---|
| Klik w warstwę | zaznaczenie; Shift/Ctrl – dodanie do zaznaczenia |
| Klik w puste miejsce ramki | zaznaczenie całego elementu (Alt+klik – element także nad warstwą) |
| Przeciąganie ramką po pustym | zaznaczanie ramką (marquee) |
| Przeciąganie warstwy | przesuwanie z przyciąganiem do krawędzi/środka ramki i innych warstw (Alt wyłącza, Shift blokuje oś) |
| Uchwyty w rogach/krawędziach | zmiana rozmiaru (Shift – zachowanie proporcji) |
| Uchwyt obrotu | obrót wokół środka warstwy |
| Zaznaczony element: przeciąganie | przesuwa layout (marginesy/pozycję) całego elementu; uchwyt skali i obrotu wokół lewego górnego rogu |
| Dwuklik w tekst | edycja tekstu na kanwie (Enter zatwierdza, Esc anuluje) |
| Kółko myszy | zoom względem kursora (0,08–8×) |
| Spacja + przeciąganie | przesuwanie widoku (pan) |
| Strzałki / Shift+strzałki | przesunięcie zaznaczenia o 1 / 10 px |

Warstwy zablokowane (🔒) nie przesuwają się. Z zaznaczoną warstwą ze ścieżką ruchu kanwa pokazuje punkty i uchwyty
Béziera ścieżki (przeciąganie edytuje ścieżkę; Alt – uchwyty niesymetryczne).

### Skróty klawiszowe

| Skrót | Akcja |
|---|---|
| Ctrl+O / Ctrl+S / Ctrl+Shift+S | otwórz / zapisz / zapisz jako |
| Ctrl+Z, Ctrl+Y (Ctrl+Shift+Z) | cofnij / ponów |
| Ctrl+C, Ctrl+V, Ctrl+D | kopiuj / wklej / duplikuj warstwy |
| Ctrl+A | zaznacz wszystkie warstwy |
| Del / Backspace | usuń zaznaczone (gdy zaznaczone są klucze na osi – usuwa klucze) |
| Esc | zatrzymaj odtwarzanie, a gdy nic nie gra – odznacz |
| F | dopasuj widok do pola |
| Ctrl+P | podgląd HTML w przeglądarce |

## 5. Animacje

### 5.1 Presety (IN / OUT)

Gotowe animacje przypisywane elementowi i każdej warstwie osobno: wjazd (*slide*), zanikanie, zoom, wyskok (*pop*),
przesłony poziome i pionowe, przycięcie, rozmycie, odwrócenia, obrót, skos, spadanie, glitch, wjazd z przeskokiem,
rozsunięcie, maska ukośna, żaluzja, rosnący pasek, rozdzielenie RGB. Parametry: kierunek, czas, opóźnienie, krzywa
(easing GSAP), stagger. „Po warstwie” łączy fazy między warstwami.

### 5.2 Efekty tekstu

Animacja całego tekstu, słów, znaków lub linii: przenikanie, rozmycie, skalowanie, przesunięcia, słowa/znaki/linie
w górę/w dół/z przycięciem, odsłonięcie, maszyna do pisania (także z kursorem), szyfrowanie (scramble), licznik.
IN i OUT osobno; opcja synchronizacji z wejściem/wyjściem belki.

### 5.3 Pętle uwagi

Pulsowanie, oddychanie, bicie serca, unoszenie, podskakiwanie, chwianie, drżenie, błysk, poświata, połysk,
przesuwane podkreślenie. Działają na wizji w stanie Stop na zegarze swobodnym (jak „Use System Time” w Viz).

### 5.4 Klucze (styl After Effects)

Na osi czasu każda warstwa (i cały element) może mieć kanały kluczy. Klucze mają wartość, czas i uchwyty:

- interpolacja: liniowa, Easy Ease (wejście/wyjście), Auto Bézier, Hold, ciągłość temporalna,
- model **prędkość/wpływ** (speed/influence) po obu stronach klucza,
- **ścieżka ruchu**: pozycja po krzywej Béziera, tangenty auto/ręczne, ruch ze stałą prędkością po łuku, roving,
- **Presety na klucze**: zamienia animacje IN/OUT na edytowalne klucze (wygląd i czas bez zmian); preset zmienia się
  na `none`, pętle i efekty tekstu zostają presetami.

Źródła ruchu (preset, klucze, ścieżka) składają się na tej samej właściwości: `tx ty rot skew blur` sumują się,
`sx sy opacity fx fy sh` mnożą się, reszta – ostatnie źródło.

### 5.5 Edytor krzywych

Pływające okno z jednym kanałem naraz: widok **Wartość** (krzywa i uchwyty Béziera: poziomo wpływ, pionowo
prędkość, Shift blokuje czas) i **Prędkość** (tylko odczyt). Przeciąganie kluczy z przyciąganiem do 1/60 s (Alt wyłącza).

### 5.6 Odtwarzanie

W pasku menu: **▶ IN**, **▶ OUT**, **⟳ Cykl** (IN → trzymaj → OUT), **■ Stop**. Podgląd w edytorze używa silnika
`playback::evaluate`, a gdy element ma klucze lub ścieżkę – silnika `btb-motion`. Podgląd rysuje efekty tekstu
i maski przybliżeniem; dokładny wygląd daje strona wyjściowa.

## 6. Eksport i integracje

| Funkcja | Jak |
|---|---|
| Zapis | Plik → Zapisz / Zapisz jako (JSON `{ "_exportVersion": 3, "elements": [...] }`), Kopiuj JSON do schowka |
| Bank / biblioteka BTB-CG | Plik → Wyślij do banku / biblioteki (`POST /api/elements`; ten sam id = aktualizacja) |
| Import z serwera | Plik → Importuj z BTB-CG… (`GET /api/state`: bank + biblioteka; starsze serwery: `db.json`) lub z pliku |
| Podgląd HTML | Ctrl+P – renderer BTB-CG w przeglądarce (wymaga renderera: import z serwera lub `BTB_CG_DIR`) |
| Strona dla OBS | `btb-cgeditor --write-motion-html element.json output-motion.html [indeks] [manual]` |

Strona dla OBS używa silnika animacji skompilowanego do WASM. Parametry adresu: `?obs=1` (źródło aktywne = TAKE,
nieaktywne = reset), `?auto=1` (TAKE po załadowaniu), `?ws=ws://host:port` (komendy JSON `take`, `off`, `reset`,
`update` z polami). Strona sprawdzona w Chromium, nie w samym OBS.

## 7. Import i konwersja kodu ręcznego

Okno importu pobiera elementy z serwera BTB-CG lub z pliku. Elementy z kodem ręcznym można skonwertować na sceny
(wymaga Node.js i katalogu BTB-CG – `BTB_CG_DIR`, domyślnie `~/Nextcloud/GitHub/BTB-CG-System-alpha`). Konwerter
odtwarza stan końcowy, domyślne efekty tekstu i animacje `onShow`; uwagi trafiają do „Uwag konwersji”. Oryginalny
kod zostaje w `codeBackup`. Znane ograniczenie: elementy o auto-szerokości po zmianie tytułu rozjeżdżają się z oryginałem.

## 8. Uruchamianie i narzędzia wiersza poleceń

```
btb-cgeditor [--open plik.json] [--new TYP] [--import adres] [--gpu auto|on|off] [--renderer glow|wgpu]
btb-cgeditor --open plik.json --shot out.png --frames 25 [--select N] [--tab element|anim|code] [--graph]
             [--phase in|hold|out:SEK] [--fit-frame] [--convert-keys]
btb-cgeditor --write-motion-html in.json out.html [indeks] [manual]
btb-cgeditor --write-preview in.json out.html [indeks]
btb-cgeditor --convert-all in.json katalog [katalog-BTB-CG]
```

Zmienne środowiskowe: `BTB_GPU`, `BTB_RENDERER`, `BTB_LANG=en`, `BTB_ENGINE=motion` (wymusza silnik btb-motion w
podglądzie), `BTB_CG_DIR`. Parametry CLI mają pierwszeństwo przed zmiennymi.

Komendy `make`: `check` (pełna weryfikacja), `fmt`, `lint`, `test`, `wasm`, `parity`, `e2e`, `convert-quality`,
`ui-shots`, `package` (paczki: `.deb`, AppImage, `.exe`/`.zip`, `.app`/`.dmg` – wynik w `dist/`).
Szczegóły i zasady pracy – w `CLAUDE.md`.

## 9. Tłumaczenia

Wszystkie teksty UI idą przez `i18n::tr` / `trf`; pliki `assets/lang/pl.json` i `en.json`. Język wybiera na razie
zmienna `BTB_LANG` (domyślnie polski). Nowy napis wymaga klucza w obu plikach; pilnują tego testy `i18n::tests`.

## 10. Architektura

| Ścieżka | Rola |
|---|---|
| `src/app.rs` | okno, menu, skróty, historia (cofnij/ponów), zapis/odczyt, import, odtwarzanie |
| `src/model.rs` | model elementu i sceny zgodny z BTB-CG (z polami `extra`) |
| `src/canvas/` | kanwa: widok (zoom/pan), trafienia, narzędzia edycji, nakładka ścieżki, edycja tekstu |
| `src/panels/` | `layers.rs`, `inspector/` (layer, element, motion, code, align), `timeline.rs`, `graph.rs` |
| `src/render.rs`, `src/anim.rs`, `src/playback.rs` | rysowanie sceny w egui i animacje presetów (wzorzec: port GSAP z BTB-CG) |
| `src/compile.rs` | scena → HTML/CSS/JS elementu BTB-CG (kontrakt pilnowany testem `compile_matches_btb_cg_scene_model`) |
| `src/motion_bridge.rs`, `src/keyconv.rs` | most `ElementAnimation → Director`; konwersja presetów na klucze |
| `src/motion_page.rs`, `assets/btb-motion-player.js` | generowanie strony wyjściowej z runtime WASM |
| `src/importer.rs`, `src/convert.rs`, `tools/convert-bridge.js` | import z serwera, konwersja kodu na scenę |
| `btb-motion/` | silnik animacji bez egui: Director / Actor / Channel / Key, easingi GSAP, efekty tekstu, ścieżka ruchu |
| `btb-motion-wasm/` | ten sam silnik jako WASM (C ABI) |

Zasady silnika (zgodność z GSAP, baking easingów, czas w sekundach, pętle na zegarze swobodnym) opisuje
[PLAN-btb-motion.md](PLAN-btb-motion.md).

## 11. Znane ograniczenia

- Strona wyjściowa: brak Local Stop; czas Stop liczony z tekstu domyślnego; test tylko w Chromium.
- Podgląd egui przybliża efekty tekstowe i maski.
- Oś czasu: brak zoomu i zaznaczania ramką; ręcznie dodawane klucze to 6 właściwości i ścieżka.
- Klucze i ścieżka działają tylko na stronie wyjściowej z WASM (nie ma ich w GSAP/`compile.rs`).
- Import: auto-szerokość elementów kodowych nie jest odwzorowana.
- Wybór języka nie ma jeszcze UI.
