# Raport: prawy panel edycyjny (inspektor) – układ funkcji

Zakres: [src/panels/inspector.rs](../src/panels/inspector.rs) (576 linii), wywołanie w `app.rs` (`SidePanel::right("inspector")`), porównanie z panelem warstw (`layers.rs`) i osią czasu (`timeline.rs`).
Stan: przegląd kodu; **zmiany z sekcji 5 wdrożone** (patrz „Status wdrożenia” na końcu).

## 1. Jak panel jest zbudowany dziś

Cztery zakładki: **Warstwa | Element | Animacja | Kod**.

| Zakładka | Zawartość |
|---|---|
| Warstwa | (zaznaczony cały element → sekcja „Ramka + Layout”, patrz 2.1) · wyrównanie do ramki / wielu warstw · nazwa · X Y W H · obrót · skos · przezroczystość · grupa · **właściwości typu** (kształt / tekst / obraz / wideo / zegar / ticker) · **Ruch warstwy** (IN, OUT, tekst IN, tekst OUT) |
| Element | nazwa · ID · typ · ramka sceny · Layout · wartości TITLE / pola · wejścia OCG · przyciski „utwórz scenę / przywróć” |
| Animacja | animacja **elementu** (IN, OUT, efekty tekstu, synchronizacja, pętla) + lista **ruchu wszystkich warstw** (`layers_motion`) |
| Kod | HTML / CSS / JS |

## 2. Co jest źle umiejscowione

Od najpoważniejszych.

### 2.1 Layout i ramka elementu występują w dwóch miejscach
`frame_section` + `layout_section` są wywoływane z `element_tab` **i** z `element_selection` (zakładka Warstwa, gdy zaznaczony jest „▣ Element”). Ten sam stan edytowany w dwóch zakładkach; użytkownik nie wie, która jest „właściwa”. Zakładka „Warstwa” zmienia przy tym znaczenie zależnie od zaznaczenia (raz warstwa, raz element), a przyciski zakładek się nie zmieniają.
**Propozycja:** wybór „Element” w panelu warstw przełącza na zakładkę Element (albo zakładka Warstwa pokazuje tylko komunikat/link). Layout mieszka w jednym miejscu.

### 2.2 Ruch warstwy edytowany w dwóch miejscach (Warstwa i Animacja)
`motion_props` jest wywołane z końca `layer_tab` oraz z `layers_motion` w zakładce Animacja (zwijane nagłówki dla każdej warstwy). Dwie ścieżki do tych samych danych, a ta w „Animacji” jest przy tym zakopana pod animacją elementu, pętlą i tekstem.
**Propozycja:** zostawić jedno miejsce. Naturalne: zakładka Animacja z przełącznikiem zakresu „Element / zaznaczona warstwa”; lista wszystkich warstw zostaje tylko dla powiązań „po warstwie” i kopiowania.

### 2.3 Zakładka „Animacja” miesza dwa poziomy
Animacja elementu (`anim_tab`: IN/OUT/pętla) i animacja warstw (`layers_motion`) mają zupełnie inną skalę, a leżą w jednym przewijanym widoku. Do tego `anim_spec` (element) i `motion_spec` (warstwa) to niemal kopie (typ, kierunek, czas, opóźnienie, ease), różnią się tylko wierszem „po warstwie”. Duplikat kodu i UI.
**Propozycja:** jedna funkcja `motion_spec` z opcjonalnym `peers`; `AnimSpec` i `MotionSpec` powinny mieć wspólny trait/konwersję.

### 2.4 Klucze i ścieżka ruchu warstwy nie mają wejścia w panelu
`Layer.keys` i `Layer.path` żyją tylko na osi czasu/graph editorze/kanwie. Inspektor pokazuje presety IN/OUT, ale nic o tym, że warstwa ma klucze (a podgląd przez to przełącza się na inny silnik: `motion_bridge::uses_keys`). Przycisk „Presety na klucze” jest na osi czasu, nie przy presetach, których dotyczy. `stopAt` (Stop) też jest tylko na osi.
**Propozycja:** w sekcji ruchu wskaźnik „Warstwa ma N kanałów kluczy / ścieżkę” + przyciski „Presety na klucze” (zakres warstwa) i „Usuń klucze”.

### 2.5 Widoczność i blokada tylko w panelu warstw
`visible` / `locked` nie występują w inspektorze (zaznaczona ukryta warstwa wygląda jak wszystkie inne). Odwrotnie: nazwa warstwy jest w inspektorze, ale nie da się jej zmienić w panelu warstw (brak zmiany nazwy w liście). Do tego `+` (duplikuj) i `×` (usuń) w wierszu są łatwe do pomylenia z klikiem w nazwę.
**Propozycja:** nagłówek inspektora: nazwa, 👁, 🔒 + dwuklik w panelu warstw = zmiana nazwy.

### 2.6 Brak podziału wg tego, co użytkownik robi
Warstwa jest jedną długą kolumną (do ~10 sekcji dla tekstu: geometria → typ → czcionka → cień tekstu → ruch IN/OUT/tekst IN/OUT). Brakuje zwijanych sekcji (Transform / Wygląd / Tekst / Ruch) i pamięci ich stanu. Wszystko, co rzadkie (squash, cień tekstu jako surowy string CSS, efekt tła, maska), jest na tym samym poziomie co X/Y.
**Propozycja:** `CollapsingHeader` dla sekcji, kolejność od najczęstszych (Transform → Wygląd/Tekst → Ruch → Zaawansowane).

### 2.7 Pola, które należą gdzie indziej
- **Wartości TITLE / pola** (`element_tab`) to dane do wpisania na antenie, a leżą pod ID, typem i layoutem; tekst tej samej wartości edytuje się też w warstwie tekstowej (`default_text` ↔ `fields`, ręczna synchronizacja w `layer_tab`). Dwa źródła prawdy dla jednej treści.
- **ID elementu i typ** (`BADGE` itd.) to metadane, a mają ten sam rząd co Layout.
- **„Dopasuj wysokość” (`text.fit_h`)** to narzędzie geometrii, a jest ukryte na dole sekcji czcionki.
- **Wyrównanie** (`align_single` / `align_tools`) leży nad nazwą warstwy; oś wyrównania do ramki dla jednej warstwy i do zaznaczenia dla wielu to różne funkcje w jednym miejscu. Wyrównanie do ramki nie uwzględnia obrotu warstwy (liczy z `x`, `w`), a kanwa już liczy z obrotem (commit 60351b7).
- **Maska** jest w `media_props`, choć maskowanie dotyczy każdej warstwy (a kształty nie mogą mieć maski w ogóle).
- **Promień narożników** (`radius_ui`) jest wołany osobno w kształcie i w obrazie/wideo; to ta sama cecha ramki warstwy, a nie ma jej np. w tekście (tło tekstu trzeba robić osobnym kształtem).

### 2.8 Wielokrotne zaznaczenie: panel edytuje tylko pierwszą warstwę
Przy `selected.len() > 1` pokazują się narzędzia wyrównania, ale reszta formularza edytuje `selected[0]` bez żadnego komunikatu. Użytkownik myśli, że zmienia wszystkie. Brak edycji zbiorczej (np. przezroczystość, kolor, przesunięcie).
**Propozycja:** dla wielu warstw pokazać tylko wyrównanie + wspólne pola (X/Y jako przesunięcie, przezroczystość), albo wyraźny nagłówek „edytujesz: <nazwa>”.

### 2.9 Zakładka „Kod” nie jest edycją warstwy
Leży obok zakładek warstwy/animacji, choć dotyczy całego elementu; dla elementów ze sceną jest tylko do odczytu. Razem z „Element” (przyciski scena/backup) tworzy dwa miejsca na tryb „ręczny kod”.
**Propozycja:** zakładka „Kod” + stan ręcznego kodu w jednym miejscu (np. pod Elementem, po zwinięciu).

## 3. Problemy techniczne w tym samym pliku

1. **Samo oglądanie zmienia model.** `get_or_insert_with` w `shape_props` / `font_props` / `media_props` / `Clock` / `Ticker` oraz `l.shadow = Some(...)`, `l.effect = Some("none")`, `l.fit`, `layer.format`, `layer.speed/gap/separator` zapisują wartości domyślne do warstwy przy każdym narysowaniu. Efekt: plik zmienia się po samym zaznaczeniu, a stan „brak ustawienia” i „ustawione na domyślne” przestają się różnić (kłopot dla diffów i dla wskaźnika „zmodyfikowano”, jeśli istnieje).
2. **Ukryte efekty uboczne `motion_spec`:** `sync_fallback(&mut s.extra, …)` wywoływane w każdej klatce rysowania.
3. **Stałe stringi w logice:** `"(dziedzicz)"` jest zarazem wartością i kluczem logiki w `optional_text` (poza i18n), hardkodowane listy czcionek, `"TITLE"`, `"none"`.
4. **Plik `inspector.rs` łączy 5 odpowiedzialności** (shell z zakładkami, warstwa, wyrównanie, ruch, element, kod). Zasada z CLAUDE.md „kod zgodny z otoczeniem” nie wymaga rozbicia, ale podział na `inspector/{layer,element,motion,align}.rs` ułatwi ruch opisany w sekcji 2. `motion_props` jest `pub`, więc i tak jest już częścią API.
5. **Wielkie linie** (do ~300 znaków) w `shape_props`, `align_tools`, `layout_section` – zgodne z resztą `src/`, ale utrudniają przegląd.
6. **Pusta linia z białymi znakami** w `layout_section` (`inspector.rs:392`), martwy kod `tmp_title` w `app.rs` (~l. 603).
7. **Przy testach UI:** `on_hover_text` na przyciskach wyrównania (`align_single` / `align_tools`) koliduje z regułą z CLAUDE.md dla testów interakcji; dziś testów tych przycisków nie ma, więc zmiana układu jest tania.

## 4. Proponowany docelowy układ

```
[ nagłówek:  <nazwa> 👁 🔒   typ ]                ← zawsze widoczny
Zakładki:  Właściwości | Ruch | Element | Kod
  Właściwości: ▸ Transform (X Y W H, obrót, skos, przezr.) + wyrównanie
               ▸ Wygląd / Tekst / Obraz … (wg typu)
               ▸ Zaawansowane (grupa, maska, efekt tła, squash)
  Ruch:        zakres [Element | Warstwa]
               ▸ IN  ▸ OUT  ▸ Tekst  ▸ Pętla  ▸ Klucze/ścieżka (wskaźnik + presety→klucze)
               ▸ Powiązania warstw / kopiuj ruch
  Element:     nazwa, ID, typ, ramka, Layout, wartości TITLE/pola, wejścia OCG
  Kod:         HTML / CSS / JS + tryb ręczny
```

## 5. Kolejność wdrożenia (od najtańszego)

1. Jedno miejsce dla Layoutu/ramki (2.1) i dla ruchu warstwy (2.2): usunięcie duplikatów, bez nowych funkcji.
2. Nagłówek z nazwą/widocznością/blokadą (2.5) i komunikat przy wielokrotnym zaznaczeniu (2.8).
3. Sekcje zwijane (2.6) + naprawa efektów ubocznych (3.1).
4. Połączenie `anim_spec` i `motion_spec` (2.3).
5. Wskaźnik kluczy/ścieżki i „Presety na klucze” w sekcji ruchu (2.4).
6. Podział pliku (3.4) – razem z krokiem 1–4, nie osobno.

Każdy krok dodaje/zmienia klucze i18n w `pl.json` **i** `en.json` (testy `i18n::tests` to pilnują) i wymaga ponownych zrzutów `make ui-shots --update`.

## 6. Status wdrożenia

Wszystkie kroki z sekcji 5 zrobione. `inspector.rs` rozbity na `src/panels/inspector/{mod,layer,align,motion,element,code}.rs`.

| Punkt | Co zrobiono |
|---|---|
| 2.1 | Layout i ramka tylko w zakładce Element; wybór „▣ Element” w panelu warstw przełącza na nią (raz, przy zmianie zaznaczenia), zakładka Właściwości pokazuje tylko podpowiedź i przycisk |
| 2.2, 2.3 | Zakładka „Ruch” z przełącznikiem Element / Warstwa; jedna `spec_grid` dla `AnimSpec` i `MotionSpec`; lista wszystkich warstw to zwinięty przegląd (wybór warstwy + kopiowanie ruchu) |
| 2.4 | Sekcja „Klucze i ścieżka”: liczba kanałów/kluczy, „Presety na klucze” (zakres element/warstwa), „Usuń klucze”. Pozycja Stop nadal tylko na osi czasu |
| 2.5 | Nagłówek inspektora: nazwa, 👁, 🔒; dwuklik w panelu warstw zmienia nazwę |
| 2.6 | Sekcje zwijane: Przekształcenie, Wygląd, Zaawansowane (grupa, efekt tła, squash, maska) |
| 2.7 | „Dopasuj wysokość” przy W/H; ID i typ elementu w „Zaawansowane”; stan kodu ręcznego/scena odłączona w zakładce Kod; wartości TITLE wyżej niż layout; wyrównanie liczone z prostokąta po obrocie (`canvas::layer_frame_bbox`) |
| 2.8 | Wiele warstw: nagłówek „Zaznaczono warstw: N”, wyrównanie oraz wspólne przesunięcie i krycie |
| 2.9 | Stan sceny/kodu w jednym miejscu (Kod) |
| 3.1, 3.2 | `edit_opt` / `edit_or`: samo rysowanie nie wpisuje wartości domyślnych (test `edit_helpers_do_not_materialize_defaults`); `sync_fallback` nie pisze, gdy wartość się nie zmieniła |
| 3.3, 3.6 | Stała `INHERIT`; usunięty martwy kod `tmp_title` i pusta linia |

Nie zrobiono (świadomie): edycja pozycji Stop w inspektorze (potrzebuje planu odtwarzania), druga ścieżka edycji wartości TITLE ↔ `default_text` (nadal ręczna synchronizacja przy edycji warstwy tekstowej).

Weryfikacja: fmt, clippy, `cargo test --workspace` (94 + testy silnika), WASM parity i testy UI przechodzą; `make e2e` zgłasza `graphic_drop_rotate` (poz. 5,3 px, próg 3 px), **to samo na czystym HEAD przed zmianami**, więc nie jest skutkiem tej zmiany. Wzorce `tests/ui/golden` odświeżone (`tools/ui-shots.py --update`).
