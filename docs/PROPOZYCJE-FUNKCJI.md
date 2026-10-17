# Propozycje funkcji do wdrożenia

Źródła: obecny stan aplikacji ([DOKUMENTACJA.md](DOKUMENTACJA.md), sekcja „Znane ograniczenia”, „Dalej” w `CLAUDE.md`) oraz klon After Effects –
EffectCraft (`~/Nextcloud/GitHub/effectcraft-main/`: `docs/ui-parity.md`, `docs/footage.md`, `crates/keyframe`, `crates/ui-egui/src/panels/`).
Serwer BTB-CG sprawdzono w `~/Nextcloud/GitHub/BTB-CG-System-alpha/` (`server.js`, `media-store.js`).

Zasada selekcji: bierzemy z After Effects to, co ma sens przy **grafice telewizyjnej odtwarzanej na żywo w OBS** (krótkie elementy,
dane zmieniane przez operatora, stabilna strona wyjściowa). Pomijamy to, co dotyczy kompozycji filmowej (3D, kamery, tracking, rotoscoping,
render do wideo, efekty pikselowe, `.aep`).

Estymaty są orientacyjne (dni robocze jednej osoby, razem z testami i dokumentacją).

## 1. Priorytety w skrócie

| # | Funkcja | Wartość dla użytkownika | Koszt | Pochodzenie |
|---|---|---|---|---|
| **A** | **Dodawanie grafik z pliku** (szczegółowy plan w sekcji 3) | bardzo duża | 4–6 dni | `footage.md`, `viewer_drop.rs`, `media-store.js` |
| B | Kopiuj/wklej klucze (z przesunięciem w czasie) | duża | 1–2 | `key_dialogs.rs`, `crates/keyframe` |
| C | Zoom i przewijanie osi czasu, zaznaczanie ramką | duża | 2–3 | `timeline.rs` |
| D | Markery i obszar roboczy (work area) | średnia | 2–3 | `markers_ui.rs` |
| E | Rodzic–dziecko warstw (parenting) | duża | 3–5 | `layer_panel.rs`, `timeline.rs` |
| F | Maski i matte (alfa/luma) wg warstwy | duża | 4–7 | `mask`, track mattes |
| G | Tryby mieszania (blend modes) | średnia | 2–4 | compositor |
| H | Wiggle i proste ekspresje | średnia | 5–8 | `crates/expr` |
| I | Biblioteka presetów animacji (zapis własnych) | duża | 3–4 | `effects_presets.rs`, `templates.rs` |
| J | Local Stop (wiele punktów zatrzymania) | duża (wymóg TV) | 4–6 | rozwój własny, plan silnika |
| K | Kształty wektorowe (pióro, ścieżka jako warstwa) | średnia | 6–10 | shape layers |
| L | Skróty AE do odsłaniania właściwości (U, P, S, R, T) | średnia | 1–2 | `ui-parity.md` |
| M | Edytor skrótów, wybór języka w UI, paleta poleceń | średnia | 2–3 | `shortcut_editor.rs`, `settings.rs` |
| N | Podgląd na żywo na stronie wyjściowej (hot reload do OBS) | duża | 2–3 | własne |
| O | Dostępność i skala UI | mała | 1–2 | `ui-parity.md` |

Sugerowana kolejność: **A → B → C → J → I → E → N → D → F → pozostałe**.

## 2. Opis propozycji

### A. Dodawanie grafik z pliku
Obecnie warstwa obrazu ma tylko pole tekstowe „Źródło” (URL lub ścieżka). Brakuje wyboru pliku, przeciągania z systemu, schowka
i bezpiecznego przenoszenia pliku na serwer/do OBS. Pełny plan: sekcja 3.

### B. Kopiuj/wklej klucze
Dziś kopiować można tylko warstwy. W AE kopiuje się zaznaczone klucze i wkleja w miejscu wskaźnika czasu, na inną właściwość lub warstwę.
Wartość: dużo szybsze budowanie animacji (np. te same klucze krycia dla wielu warstw). Zakres: schowek kluczy w `timeline.rs`,
wklejanie w czasie wskaźnika Stop/kursora, sprawdzenie zgodności właściwości (`tx` ↔ `tx`), Ctrl+C/V gdy fokus jest na osi czasu,
odwrócenie czasu klatek (`Channel::time_reverse` już istnieje). Testy: symulowane zdarzenia egui.

### C. Zoom i nawigacja osi czasu
Brak zoomu i zaznaczania ramką to znane ograniczenie. Zakres: przybliżanie kółkiem z Ctrl, przewijanie poziome, „dopasuj do całości”,
zaznaczanie ramką kluczy (rubber-band), przyciąganie do klatki i do innych kluczy.

### D. Markery i obszar roboczy
Markery z nazwą na osi (np. „tekst wchodzi”), skok do markera, obszar roboczy ograniczający podgląd cyklu. Markery mogą też służyć
jako punkty przyciągania i jako przyszłe punkty Local Stop (patrz J).

### E. Rodzic–dziecko warstw (parenting)
Warstwa może mieć rodzica; transformacja rodzica dziedziczy się (przesunięcie, skala, obrót). Typowy przypadek: akcent i tekst
poruszają się razem z tłem bez duplikowania kluczy. Wymaga: pole `parent` w modelu warstwy, kompozycji macierzy w `render.rs` i w
`motion_bridge`/`btb-motion-player.js` (zmiana obu naraz), nowego pola w `compile.rs` (kontrakt z `scene-model.js` BTB-CG – ryzyko:
model sceny jest współdzielony; do rozstrzygnięcia z BTB-CG).

### F. Maski i matte
Warstwa jako maska innej (alfa lub luma, odwrócona). W modelu istnieje już `mask_layer`; brakuje UI do wyboru, trybów i animacji.
Zakres: lista wyboru w sekcji „Zaawansowane”, podgląd w egui, zgodność strony wyjściowej.

### G. Tryby mieszania
Normal, Add, Multiply, Screen, Overlay itp. dla warstw – efekty świetlne (poświata, połysk). CSS `mix-blend-mode` zapewnia wsparcie
w OBS; w egui potrzebne przybliżenie lub własny mesh z trybem.

### H. Wiggle i ekspresje
Najpierw jedna, bezpieczna funkcja: `wiggle(freq, amp)` jako parametrizowany kanał (suwak amplitudy i częstotliwości) bez ogólnego
interpretera. Następnie ewentualnie małe ekspresje (`time`, `value`, `linear`, `ease`, `loopOut`). Interpreter przenosimy z `crates/expr`
dopiero po decyzji o zakresie; wymaga tego samego wyniku w WASM.

### I. Biblioteka presetów animacji
Zapis zestawu kluczy/ruchu warstwy lub elementu jako preset (plik JSON w katalogu użytkownika), lista presetów z podglądem,
zastosowanie przeciągnięciem lub dwuklikiem. Pozwala dzielić animacje między elementami i operatorami. Warto zachować zgodność z
`animation` elementów BTB-CG.

### J. Local Stop
Wiele punktów zatrzymania (np. belka pokazuje nazwisko, potem funkcję) w jednym elemencie sterowanych komendą „następny”. Zamyka
znane ograniczenie strony wyjściowej. Zakres: markery typu Stop (D), osobny czas per Director, komenda `next` w odtwarzaczu i WS,
pole w panelu operatora BTB-CG. Wymaga uzgodnienia protokołu z BTB-CG.

### K. Kształty wektorowe
Warstwa ze ścieżką (pióro, wielokąt, gwiazda), wypełnienie i obrys, animacja „trim paths” (rysowanie linii). Duży koszt, ale daje
efekty, których nie ma w prostokątach.

### L–M. Ergonomia
Skróty AE: **U** (odsłoń animowane właściwości), **P/S/R/T** (pozycja/skala/obrót/krycie), strzałki z Alt, **J/K** (poprzedni/następny klucz),
paleta poleceń (Ctrl+K), wybór języka w menu Widok, edytor skrótów.

### N. Podgląd na żywo w OBS
Przycisk „Wyślij do OBS” zapisujący stronę i odświeżający źródło po zapisie (WebSocket `?ws=` już istnieje) – iteracja bez ręcznego
generowania `output-motion.html`.

### O. Dostępność
Skala interfejsu 75–200%, kontrast, nawigacja klawiaturą po panelach.

## 3. Plan: dodawanie grafik z pliku

### 3.1 Cel

Użytkownik dodaje obraz (logo, ikonę, zdjęcie) z dysku jednym ruchem i element działa wszędzie: w edytorze, w podglądzie HTML,
na stronie dla OBS oraz po wysłaniu do banku/biblioteki serwera BTB-CG.

### 3.2 Stan obecny (ustalony z kodu)

- `Layer.src: Option<String>` i `Layer.bind` (pole, np. `LOGO_URL`) – model obrazu i wideo ([model.rs:336](../src/model.rs)).
- Edytor: pole tekstowe „Źródło” w `media_props` ([layer.rs](../src/panels/inspector/layer.rs)); brak przycisku wyboru pliku.
- Render w egui ([render.rs:162-200](../src/render.rs), `draw_image`): `Values::resolve` dopuszcza `http(s)://`, `file://`, `data:`, ścieżkę
  bezwzględną (jako `file://`) i ścieżkę względną względem adresu serwera (`base_url`). Obrazy `http(s)` są pobierane w wątku roboczym.
- `compile.rs` wstawia `src` do `<img src=…>` bez zmian; `motion_page.rs` buduje stronę z `base_url: ""`, więc `/uploads/…` albo
  `file://…` **nie zadziałają w OBS** poza maszyną autora.
- `rfd::FileDialog` jest już używane (otwórz/zapisz JSON).
- Serwer BTB-CG: `POST /api/upload` (multipart, pole `file`, obrazy i `video/webm|mp4|ogg`, do 200 MB) zwraca `{ "url": "/uploads/<nazwa>" }`,
  pliki serwowane z `/uploads`. `media-store.js` przenosi przy zapisie stanu data URI do `uploads/<sha1>.<ext>`, więc **serwer akceptuje
  osadzone `data:` i sam je zewnętrzni** (komentarz w `CLAUDE.md` BTB-CG: „Pliki trzymamy jako pliki, nie kod”).

### 3.3 Decyzja projektowa: gdzie leży plik

Trzy tryby zapisu źródła, wybierane jawnie:

| Tryb | Wartość `src` | Zalety | Wady |
|---|---|---|---|
| **Osadzony** (domyślny dla eksportu do pliku i OBS) | `data:image/png;base64,…` | element samowystarczalny, działa wszędzie | duży JSON; serwer sam zewnętrzni przy wysyłce |
| **Serwerowy** | `/uploads/<plik>` (po `POST /api/upload`) | mały JSON, wspólny zasób | wymaga połączenia z serwerem |
| **Zewnętrzny** (jak dziś) | `http(s)://…` | brak kopii | zależność od cudzego hosta |

Domyślnie: po wybraniu pliku edytor **osadza** go (data URI), a przy „Wyślij do banku/biblioteki” serwer przenosi do `uploads/` (nie wymaga zmian
po stronie serwera) lub edytor wcześniej woła `/api/upload` i podmienia na `/uploads/…` (patrz etap 4). Ścieżka lokalna (`file://`) pozostaje tylko
jako tryb awaryjny z ostrzeżeniem „plik nie będzie dostępny na innych komputerach”.

Kopię oryginalnej nazwy zachowujemy w `extra` warstwy (`srcName`), by pokazać ją w panelu i użyć jako nazwy warstwy.

### 3.4 Zakres funkcjonalny

1. **Przycisk „Wybierz plik…”** obok pola „Źródło” (warstwa obraz i wideo), filtr: png, jpg, jpeg, webp, gif, svg, bmp; dla wideo webm, mp4.
2. **Przycisk w panelu warstw „Dodaj → Obraz z pliku…”**: tworzy warstwę `Image` od razu z plikiem, o rozmiarze obrazu (wpasowanym do ramki,
   z zachowaniem proporcji), wyśrodkowaną, nazwaną jak plik.
3. **Przeciągnij i upuść** plik(i) z systemu na kanwę (egui `dropped_files`) – nowa warstwa w miejscu upuszczenia; upuszczenie na istniejącą
   warstwę obrazu **podmienia** źródło (z Alt – dodaje nową warstwę).
4. **Wklejanie obrazu ze schowka** (Ctrl+V, gdy w schowku jest obraz, a nie warstwy).
5. **Wiele plików naraz** → wiele warstw, ułożonych kaskadowo lub w siatce.
6. **Podmień / usuń plik** w panelu warstwy, „Pokaż nazwę oryginału”, ostrzeżenie o rozmiarze (> 2 MB: sugestia kompresji).
7. **Pole (bind)**: przy dodaniu logo opcja „Zmienne na antenie” – ustawia `bind` na `LOGO_URL`, a plik jest wartością domyślną w `fields`
   (operator może potem podać inny adres w panelu BTB-CG).
8. **Obsługa błędów**: nieobsługiwany format, uszkodzony plik, zbyt duży plik – komunikat na pasku statusu i nic się nie zmienia.

Poza zakresem pierwszej wersji: sekwencje obrazów, PSD/AI/PDF (z `footage.md` EffectCraft), wideo z miniaturą, edycja obrazu.

### 3.5 Etapy wdrożenia

**Etap 1 – wybór pliku i osadzanie (1,5 dnia)**
- Nowy moduł `src/media.rs`: `load_image_file(path) -> Result<MediaAsset, String>` (odczyt, wykrycie typu po zawartości, limit rozmiaru,
  wymiary przez `image` crate, kodowanie `data:` base64 – `base64` jest już zależnością), `mime_for(ext)`, `file_name_to_layer_name`.
- Włączenie w `Cargo.toml` odpowiednich formatów `image` (obecnie tylko `png`): jpeg, webp, gif, bmp; SVG obsługuje loader egui_extras – sprawdzić
  w teście, że egui rysuje data URI każdego formatu (`try_load_texture` z `data:`).
- `media_props`: przycisk „Wybierz plik…” (`rfd::FileDialog`), zapis w `l.src` + `extra.srcName`.
- Testy: jednostkowe `media.rs` (mime, limit, base64 round-trip), test egui z wybraną ścieżką symulowaną bez dialogu (funkcja `apply_media(layer, asset)`
  wywoływana bezpośrednio).
- Tłumaczenia: klucze `media.pick`, `media.pick.filter`, `media.too_big`, `media.unsupported`, `media.read_error` w `pl.json` i `en.json`.

**Etap 2 – nowa warstwa z pliku i kilka plików (1 dzień)**
- `layers.rs`: `add_image_layer(scene, asset, at)` – rozmiar obrazu przeskalowany tak, by mieścił się w ramce (max 80% ramki), wyśrodkowany lub w
  punkcie upuszczenia; nazwa z pliku; zaznaczenie nowej warstwy.
- Przycisk „🖼 Obraz z pliku…” w panelu warstw (obok przycisków typów) i pozycja menu *Plik → Dodaj obraz…*.
- Wielokrotny wybór (`pick_files`) → kolejne warstwy z przesunięciem 24 px.
- Historia (cofnij/ponów) obejmuje dodanie; test cofnięcia.

**Etap 3 – przeciągnij i upuść oraz schowek (1 dzień)**
- W `app.rs`: obsługa `ctx.input(|i| i.raw.dropped_files)` – tylko gdy kursor jest nad kanwą; pliki z `path` (Linux/Windows/macOS) lub z `bytes` (web).
  Podświetlenie kanwy podczas `hovered_files`.
- Upuszczenie na warstwę obrazu → podmiana źródła (z zachowaniem rozmiaru i `fit`), z Alt → nowa warstwa.
- Wklej ze schowka: `arboard` (nowa zależność, mała) lub egui `copied_image`; jeśli zależności nie chcemy – etap opcjonalny. Decyzja przy implementacji.
- Uwaga z EffectCraft: upuszczanie plików bywa zawodne na Wayland (#268); sprawdzić na docelowym systemie i zawsze mieć przycisk jako ścieżkę podstawową.
- Test: syntetyczne zdarzenie `dropped_files` w `egui::Context::run` (istniejący wzorzec testów interakcji).

**Etap 4 – serwer, eksport i OBS (1–1,5 dnia)**
- `importer.rs`: `upload_blocking(base, name, bytes) -> Result<String, String>` (`POST {base}/api/upload`, multipart `file`; `ureq` 2 nie ma
  wbudowanego multipart – złożyć ciało ręcznie z granicą lub dodać `ureq_multipart`; wybrać wariant ręczny, bez zależności).
- „Wyślij do banku/biblioteki”: dla każdej warstwy z `data:` pytanie/opcja „Prześlij pliki na serwer” (domyślnie włączona): upload → podmiana
  `src` na zwrócony `/uploads/…` w **kopii** elementu wysyłanej na serwer (lokalny plik zachowuje osadzony obraz). Rezygnacja: serwer i tak zewnętrznizuje
  `data:` przez `media-store.js`.
- `motion_page.rs`: strona dla OBS zawsze z **osadzonymi** obrazami (data URI), by działała z pliku; jeśli `src` wskazuje `/uploads/…` a `base_url` jest znany,
  pobrać i osadzić przy generowaniu strony.
- `compile.rs`/`preview.rs`: bez zmian, `data:` przechodzi przez `esc()` – test, że apostrofy i znaki w base64 nie psują atrybutu.
- Zmiana kontraktu: żadna (pole `src` ma już obecne znaczenie), więc `compile_matches_btb_cg_scene_model` ma pozostać zielony.
- Test e2e: scena z obrazem osadzonym → `--write-motion-html` → Chromium (Playwright), obraz widoczny (`naturalWidth > 0`).

**Etap 5 – dopracowanie (0,5–1 dnia)**
- Ostrzeżenia: duży plik (> 2 MB), tryb `file://`; kompresja opcjonalna (zmniejszenie do rozmiaru ramki × 2 przy PNG/JPEG).
- Podgląd miniatury w panelu warstwy, przycisk „Podmień…”, „Usuń plik”.
- Dokumentacja: aktualizacja `DOKUMENTACJA.md` (sekcje 3, 4, 6) i `INSTRUKCJA-poczatkujacy.md` (nowy krok „Dodaj logo”), zgodnie z zasadą w `CLAUDE.md`.

### 3.6 Ryzyka i decyzje do podjęcia

| Ryzyko / pytanie | Propozycja |
|---|---|
| Rozmiar JSON z osadzonym obrazem | limit ostrzegawczy 2 MB, kompresja opcjonalna, wysyłka zewnętrznizowana przez serwer |
| `file://` nie działa na innych maszynach | tryb awaryjny z ostrzeżeniem, domyślnie osadzanie |
| Wayland i upuszczanie plików | przycisk jako podstawowa ścieżka, drop jako dodatek |
| SVG ze skryptami | w egui rysujemy rasteryzując (bez skryptów); w HTML `<img>` nie uruchamia skryptów; mimo to ostrzeżenie przy imporcie SVG |
| Wideo (`video`) | w pierwszej wersji tylko wybór pliku i ścieżka serwerowa (duże pliki nie powinny być osadzane); osadzanie wideo wyłączone |
| Multipart bez nowej zależności | złożyć ciało ręcznie (kilkanaście linii) i pokryć testem względem lokalnego serwera atrapy |
| Zgodność z operatorem BTB-CG (`bind` / `LOGO_URL`) | sprawdzić, czy panel operatora nadpisuje `src` wartością pola; w razie potrzeby dodać `fields.LOGO_URL` |

### 3.7 Kryteria akceptacji

1. Z menu lub przycisku można wybrać plik PNG/JPEG/WebP/GIF/SVG i zobaczyć go na kanwie jako nową warstwę wpasowaną w ramkę.
2. Przeciągnięcie pliku z menedżera plików na kanwę dodaje warstwę; na warstwę obrazu – podmienia źródło.
3. Zapisany JSON po ponownym otwarciu na innym komputerze pokazuje ten sam obraz.
4. `--write-motion-html` tworzy stronę, która wyświetla obraz w Chromium bez dostępu do oryginalnego pliku i serwera.
5. „Wyślij do banku BTB-CG” kończy się elementem z obrazem widocznym na serwerze (`/uploads/…`) i w jego podglądzie.
6. Cofnij/ponów działa dla dodania i podmiany; błędny plik daje komunikat bez zmiany sceny.
7. `make check` zielony; testy: jednostkowe `media.rs`, interakcji egui (wybór, drop, podmiana), e2e Chromium; nowe klucze w obu plikach językowych;
   dokumentacja zaktualizowana w tym samym commicie.

### 3.8 Podział na commity

1. `media.rs` + testy (silnik pliku) · 2. UI wyboru pliku w inspektorze + tłumaczenia · 3. nowa warstwa z pliku, menu, historia ·
4. drop i schowek · 5. upload na serwer i osadzanie w stronie OBS + test e2e · 6. dopracowanie i dokumentacja.
Każdy commit: `make check` (lub jego odpowiedni fragment) i aktualizacja dokumentacji zgodnie z `CLAUDE.md`.

## 4. Czego świadomie nie bierzemy z After Effects

3D, kamery i światła, tracking i Roto Brush, efekty pikselowe (306 w EffectCraft), render do wideo/ProRes, kolejka renderowania, import `.aep`,
Puppet, malowanie, skrypty ExtendScript. Wynikają z innego celu (kompozycja filmowa), a wynik działa tylko na stronie wyjściowej HTML/WASM.

## 5. Wpływ na dokumentację i testy

Każda z funkcji wymaga: testów w stylu istniejących (symulowane zdarzenia egui, parytet WASM dla zmian silnika, `e2e` dla strony wyjściowej),
kluczy w `pl.json`/`en.json`, aktualizacji `DOKUMENTACJA.md` i instrukcji oraz wpisu w „Znane ograniczenia”, jeśli któreś zostaje zamknięte.
