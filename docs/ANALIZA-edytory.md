# Analiza edytorów: krzywych i kanwy

Stan na kod z gałęzi `main`. Analiza z czytania kodu (bez uruchamiania). Dokument opisuje logikę i proponuje kierunki rozbudowy.

---

## 1. Edytor krzywych (`src/panels/graph.rs`)

### Rola
Pływające okno egui (checkbox na osi czasu, `tl.graph_open`) pokazujące **jeden kanał kluczy naraz** w widoku *Wartość* albo *Prędkość*. Zawiera wyłącznie logikę UI; matematyka uchwytów i krzywych jest w `btb-motion` (`Channel::eval`, `velocity`, `handle_point`, `set_handle_point`, `move_keys`, `curve.rs`).

### Wybór kanału
- Fokus `g.focus = (warstwa, właściwość)` pochodzi z pierwszego zaznaczonego klucza na osi czasu (`tl.sel`); combo w nagłówku zmienia go i czyści zaznaczenie.
- Cel: warstwa (`Layer.keys`, opcjonalnie `Layer.path`) albo cały element (`ELEMENT`, klucze w `Scene.keys`).
- Ścieżka ruchu jest pseudo-kanałem `PATH`: wykres pokazuje `distance_channel()` (długość łuku w czasie), więc jego prędkość = prędkość ruchu po krzywej.

### Rysowanie
1. 161 próbek kanału w zakresie [pierwszy klucz − 10%, ostatni klucz + 10%].
2. Zakres wartości z `fit()`: klucze i punkty uchwytów (widok wartości) lub próbki prędkości + zero; zapas 12%, płaski wykres ±1.
3. Siatka (5 linii poziomych, pionowe co 0,5 s), krzywa (zielona = wartość, niebieska = prędkość), klucze, uchwyty (żółte) tylko dla zaznaczonych kluczy i tylko w widoku wartości.
4. `View` mapuje (czas, wartość) ↔ ekran; ma test round-trip.

### Interakcja
- **Klucz**: klik zaznacza; przeciąganie startuje z migawki kanału (`GDrag::Key`), czas przyciągany do siatki 1/60 s (Alt wyłącza), `move_keys` zwraca nowy kanał i nowe indeksy (klucze mogą zmienić kolejność). Klucz ścieżki (`GDrag::PathKey`) przesuwa się tylko w czasie, bo wartość wynika z geometrii.
- **Uchwyt**: pozycja kursora → (t, v) → `set_handle_point(i, out, t, v, shift)`: poziomo wpływ, pionowo prędkość, Shift blokuje czas. Dla ścieżki edycja idzie przez `distance_channel` i wraca przez `apply_distance_handles`.
- **Widok prędkości** jest tylko do odczytu (`speed_view_active`).
- Model uchwytów (`curve.rs`): `Linear`, `Smooth`, `Free`, `Hold`, `Auto`, `Ease{speed, influence}`; segment to sześcienna krzywa Béziera z przycinaniem uchwytów (krzywa zostaje funkcją czasu), wartość dla t przez bisekcję (32 iteracje).

### Testy
Symulowane zdarzenia egui, klikanie w zapisane `key_pos` / `handle_pos`: uchwyty, Shift, przeciąganie klucza, zmiana zaznaczenia, brak edycji w widoku prędkości, ścieżka, kanał elementu.

### Słabe punkty
- Brak zoomu i panoramy; zakres osi liczony co klatkę, więc podczas przeciągania skala Y „pływa".
- Zaznaczenie jednokrotne (`tl.sel = vec![jeden]`): brak Shift+klik, ramki, grupowego przesuwania.
- Widok prędkości tylko do odczytu, mimo że `set_ease` istnieje.
- Jeden kanał naraz; brak nakładania np. `tx` i `ty`.
- Brak narzędzi interpolacji (Easy Ease, Hold, Linear, Auto) na wykresie, menu kontekstowego, dwukliku wstawiającego klucz.
- Brak playheada i scrubbingu na wykresie; brak jednostek na osiach i liczb prędkości/wpływu przy uchwycie.
- 161 wywołań `eval`/`velocity` na klatkę; okno pływające, `PLOT_H = 240` na sztywno.

### Propozycje (od najtańszych)
1. **Zoom i panorama** (kółko = czas, Ctrl+kółko = wartość, środkowy przycisk = przesuwanie, „dopasuj"); zakres w `GraphUi` zamiast co klatkę (naprawia też pływającą skalę). Wzorzec: `reference/effectcraft/crates/ui-egui/src/panels/graph_tools.rs`.
2. **Playhead i scrubbing** na wykresie (czas z `Playback`).
3. **Zaznaczanie wielu kluczy** (Shift+klik, ramka, ruch grupowy); `move_keys(&[idx])` już to przyjmuje.
4. **Narzędzia interpolacji** na pasku wykresu (`easy_ease`, `set_ease`, Hold, Linear, Auto) i skróty (F9).
5. **Edycja w widoku prędkości** (uchwyty w przestrzeni prędkości; `Handle::Ease` ma prędkość i wpływ wprost).
6. **Wiele kanałów naraz** (kolory, „pokaż zaznaczone / wszystkie").
7. **Dwuklik wstawia klucz, Delete usuwa** (`insert_key`, `remove_keys`).
8. **Biblioteka krzywych referencyjnych** z `ease.rs` jako nakładka i „zastosuj" przez `Channel::fit`.
9. **Panel dokowany** zamiast okna; kopiuj/wklej klucze.
10. **Prędkość przestrzenna ścieżki** z markerami roving i przypięciem do geometrii na kanwie.

Kolejność: 1 → 2 → 4 → 3, potem 5 i 6.

---

## 2. Kanwa (`src/canvas.rs`)

### Rola
Interaktywny podgląd sceny: widok (zoom/pan), zaznaczanie, przesuwanie / skalowanie / obrót warstw z uchwytami, przyciąganie do prowadnic, zaznaczanie ramką, nakładka ścieżki ruchu, edycja tekstu w miejscu z paskiem formatowania i pasek dodawania warstw. Jedna funkcja `draw_canvas` (ok. 475 linii) wywoływana z `app.rs:676`; stan w `CanvasState`. Właściwe rysowanie sceny robi `render::draw_scene`.

### Układy współrzędnych
- **Pole (field)**: stałe 1920×1080 (`FIELD_W/H`). **Ramka elementu (frame)**: `scene.frame`, położona w polu w `frame_origin` (`fo`, z `Layout::frame_origin`), narysowana żółtą linią przerywaną.
- Warstwa ma `x, y, w, h, rotation`. `static_aff` daje macierz warstwy → pole bez animacji (transformy domyślne), `view_aff = view.aff() * translate(fo)` przenosi do ekranu. `to_frame(p)` = ekran → współrzędne ramki.
- Punkty ścieżki ruchu to przesunięcia od środka warstwy (`path_center`).

### Widok
- Zoom kółkiem względem kursora (0,08–8×, krok 0,16% na jednostkę przewinięcia), pan środkowym / prawym przyciskiem lub Spacja+LPM.
- `needs_fit` (całe pole, margines 4%) i `needs_fit_frame` (ramka elementu, 85% obszaru) ustawiane z zewnątrz.
- Tło: szachownica / ciemne / zielone / szare / białe; pola bezpieczne 90% i 80%.

### Interakcja (maszyna stanów `Drag`)
Kolejność rozstrzygania przy `drag_started`: **klucz/uchwyt ścieżki → uchwyt skalowania → uchwyt obrotu → warstwa → ramka zaznaczenia**.
- **Move**: migawka pozycji zaznaczonych niezablokowanych warstw; Shift blokuje oś; przyciąganie (próg 6 px ekranu / zoom) bounding boxa grupy (lewo/środek/prawo, góra/środek/dół) do krawędzi i środków ramki oraz innych widocznych warstw, Alt wyłącza; prowadnice różowe; pozycje zaokrąglane do pikseli.
- **Resize**: 8 uchwytów, obliczenia w lokalnym układzie obróconej warstwy względem środka początkowego, przeciwległy róg/krawędź zostaje nieruchomy; Shift zachowuje proporcje na rogach; minimum 1 px.
- **Rotate**: uchwyt 26 px nad górną krawędzią, kąt względem środka, Shift = krok 15°, zaokrąglenie do 0,1°.
- **Marquee**: zaznacza warstwy, których bounding box po obrocie przecina prostokąt.
- **Ścieżka** (tylko przy jednej zaznaczonej warstwie, nie w trakcie odtwarzania): przeciąganie klucza (siatka 0,5 px, Shift blokuje oś), przeciąganie tangenta (węzeł gładki → drugi symetryczny, Alt rozdziela, ręczny tangent wyłącza `auto`). Klik w klucz ustawia `path_picked`, które aplikacja przenosi do osi czasu.
- Klik: wybór warstwy (Shift/Ctrl przełącza); dwuklik na warstwie tekstowej uruchamia edycję w miejscu (`TextEdit` + pływający pasek: rodzina, rozmiar, B/I, wyrównanie, kolor; Enter / klik poza zatwierdza, Esc anuluje).
- Pasek dodawania warstw w lewym górnym rogu (jak w Canvie): klik = nowa warstwa na środku; tekst od razu wchodzi w edycję.
- Podczas odtwarzania (`playing`) interakcja jest wyłączona.

### Testy
Symulowane zdarzenia egui dla nakładki ścieżki: widoczność kluczy/uchwytów, przeciąganie klucza (nie rusza warstwy), uchwyt (wyłącza auto, symetria, Alt), klik w klucz bez zmiany zaznaczenia warstw. **Brak testów** dla Move/Resize/Rotate/Marquee/przyciągania, zoomu i edycji tekstu.

### Zauważone słabe punkty
- Funkcja ma ok. 475 linii i miesza hit-testing, stan przeciągania, rysowanie i UI; trudna do testowania i rozbudowy.
- Przyciąganie liczy bounding box z `l.x/l.w` bez obrotu, a prowadnice od innych warstw też ignorują obrót; skalowanie ma martwe pole `start_mouse` (`let _ = start_mouse`).
- Marquee nadpisuje zaznaczenie nawet z Shift/Ctrl (brak dodawania do zaznaczenia); trafienie bounding boxem, nie kształtem.
- Hit-test omija tylko niewidoczne warstwy; zablokowane warstwy nadal przechwytują klik (brak „przeklikiwania" do warstwy pod spodem), brak Alt+klik do wyboru warstwy pod spodem.
- Przy wielu zaznaczonych warstwach brak wspólnych uchwytów (skalowanie/obrót grupy tylko dla jednej warstwy).
- Pole 1920×1080 jest stałe; brak linijek, własnych prowadnic, siatki, strzałek do przesuwania o 1 px (w tym module).
- Przesuwanie punktu odniesienia (pivot) nie istnieje; obrót i skala zawsze względem środka.
- Zoom używa `smooth_scroll_delta` (zbędne `raw_scroll_delta * 0.0`); brak pinch-zoom i dopasowania skrótem.
- Edycja tekstu to `singleline` (wielolinijkowość i formatowanie fragmentów niedostępne), kolejny pasek `Area` poza ramkami kanwy.
- Nakładka ścieżki tylko dla pojedynczej warstwy; brak podglądu pozycji w czasie (duchy, znaczniki klatek).
- Brak minimapy / wskaźnika przy powiększeniu; brak widoku „1:1" (100%) i rzeczywistego podglądu klipowania do ramki.

### Propozycje rozbudowy
1. **Refaktor** `draw_canvas` na moduły: `view` (nawigacja), `hit` (picking), `tools` (Move/Resize/Rotate/Marquee/Path jako osobne struktury z `fn begin/update/end`), `overlay` (rysowanie). Odblokowuje testy i dalsze funkcje.
2. **Testy** brakujących narzędzi (Move z przyciąganiem, Resize z obrotem, Rotate z Shift, Marquee, zoom wokół kursora).
3. **Poprawne przyciąganie dla warstw obróconych** (bounding box po obrocie) i dla zmiany rozmiaru/obrotu; dodatkowo odległości równe (smart guides z etykietami) i przyciąganie do pól bezpiecznych.
4. **Linijki, własne prowadnice** (przeciąganie z linijki), siatka z przyciąganiem, strzałki (1 px, Shift = 10 px), wyrównywanie/dystrybucja zaznaczonych warstw.
5. **Zaznaczanie wielu warstw**: wspólna ramka z uchwytami (skala/obrót grupy), Shift/Ctrl+marquee dodaje do zaznaczenia, Alt+klik wybiera warstwę pod spodem, Ctrl+klik wchodzi „w głąb" niezależnie od blokady.
6. **Pivot / punkt zakotwiczenia** przeciągany na kanwie (wymaga pola w modelu i obsługi w silniku oraz `btb-motion-player.js`).
7. **Edycja tekstu**: pole wielolinijkowe, automatyczne dopasowanie do rozmiaru warstwy, podgląd faktycznego renderu, skróty (Ctrl+B/I).
8. **Nawigacja**: skróty (1 = 100%, 0 = dopasuj, F = zaznaczenie), pinch-zoom, minimapa, przybliżenie do zaznaczenia.
9. **Podgląd ruchu**: duchy w kilku momentach czasu (onion skin), znaczniki klatek na ścieżce, edycja tangentów kilku kluczy, ruch całej ścieżki przeciągnięciem.
10. **Klawisze i schowek**: Ctrl+D (duplikuj), Ctrl+C/V warstw (i kluczy), Delete, kolejność (Ctrl+[ ]), blokada/ukrycie z kanwy.
11. **Warstwy-kształty z uchwytami**: zaokrąglenie narożników, przeciąganie kolorów gradientu, uchwyty obrysu bezpośrednio na kanwie.

Kolejność: 1 → 2 → 3 → 5 → 4, potem 6–8.

---

## 3. Wspólne wnioski

- Oba edytory mają ten sam wzorzec testów (symulowane zdarzenia egui, zapamiętane pozycje klikanych punktów) i tę samą zasadę „bez `on_hover_text` na klikanych widżetach"; warto go wyciągnąć do wspólnego `test_support` (np. `drag`, `click`, `frame`).
- Zaznaczenie kluczy ścieżki i kanałów jest rozproszone: `CanvasState.path_sel`, `path_picked`, `TimelineUi.sel`. Jedno wspólne źródło prawdy (`Selection`) uprościłoby synchronizację osi czasu, wykresu i kanwy.
- Wielokrotne zaznaczanie to wspólny brak obu edytorów; ruch grupowy można zaprojektować raz i użyć w kanwie, osi czasu i wykresie.
