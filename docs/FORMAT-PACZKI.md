# Paczka elementu (`.cgpack`) – projekt formatu

Status (2026-10-17): **wdrożone** — format `.cgpack` v1 (rozszerzenie i biblioteki zatwierdzone w planie M1: `zip` + `sha1` w Rust, `fflate` w Node).
Edytor: `src/pack.rs` (zapis/odczyt, walidacja, testy, obieg z `pack-io.js` w BTB-CG), menu Plik → „Eksportuj paczkę elementu (.cgpack)…”, otwieranie `.cgpack` (Otwórz, `--open`).
System: `pack-io.js`, `POST /api/packs`, `GET /api/packs/:id`, przyciski w panelu i Trio. Wysyłka z edytora na serwer nadal idzie przez `POST /api/elements` z obrazami jako `data:`
(serwer sam zewnętrznizuje do `uploads/` — wariant dopuszczony w §4); osobna wysyłka paczki z edytora nie jest potrzebna. Wideo w paczce: obsługiwane przez serwer, edytor czyta tylko obrazy.
Cel: jeden plik wymienialny między **BTB CG Editor** (Rust) i **BTB CG System** (repo `BTB-CG-System-alpha`, gałąź `TRIO`), zawierający
element wraz z jego plikami graficznymi.

## 1. Po co

Dziś element to JSON `{ "_exportVersion": 3, "elements": [...] }`. Obrazy są w nim albo jako adresy (`http(s)://…`, `/uploads/…` – zależne od
serwera), albo osadzone jako `data:` base64 (działa, ale JSON puchnie o 1/3, a serwer BTB-CG i tak przenosi je do `uploads/` – `media-store.js`).
Paczka rozwiązuje to: **dane ustawień w czytelnym JSON + składniki jako zwykłe pliki** (PNG, SVG…), całość w jednym archiwum.

## 2. Nazwa i rozszerzenie

- Rozszerzenie: **`.cgpack`**, typ MIME `application/vnd.btb.cgpack+zip` (to archiwum ZIP – można je rozpakować zwykłym programem).
- W BTB-CG istnieje już „Paczka CG” (`_type: "cg_package"`, JSON z całą biblioteką i bankiem – `exportCGPackage`). To **inna rzecz**: cały zestaw
  antenowy. Aby uniknąć pomyłek: nowy typ nazywamy **„paczka elementu”** (`_type: "cg_element_pack"`), a w UI wprost: „Eksportuj paczkę elementu (.cgpack)”.
  Nazwa rozszerzenia jest do zatwierdzenia (alternatywy: `.cgel`, `.btbcg`).

## 3. Struktura archiwum

```
belka-glowna.cgpack            (ZIP, kompresja deflate, nazwy UTF-8)
├── manifest.json              nagłówek paczki (wersja, lista plików, sumy)
├── element.json               element: { "_exportVersion": 3, "elements": [ { … } ] }
└── assets/
    ├── 3f2a…c1.png            składniki graficzne: <sha1>.<rozszerzenie>
    └── 9b10…e7.svg
```

Zasady:

1. **`element.json` jest zwykłym eksportem v3** (ten sam, który czytają `EditorIO.parse` i `importer::parse_elements`), z jedną różnicą: w polach
   `src` warstw obrazu zamiast URL/`data:` stoi **ścieżka względna** `assets/<sha1>.<ext>`.
2. **Nazwa pliku = SHA-1 zawartości** (jak w `media-store.js`: `uploads/<sha1>.<ext>`). Ta sama grafika użyta dwa razy = jeden plik; import na
   serwer jest idempotentny i bez duplikatów.
3. Pliki wymienione w `manifest.json` z rozmiarem i sumą SHA-1; import weryfikuje sumę i odrzuca niezgodne.
4. W paczce **jeden element** (wersja 1). Wiele elementów = kolejna wersja formatu (`elements` i tak jest tablicą, więc czytniki muszą ją znosić).
5. Nieznane pola zachowujemy (zasada `extra` z modelu edytora) – import → eksport nie gubi danych.

### `manifest.json`

```json
{
  "_type": "cg_element_pack",
  "_packVersion": 1,
  "exportVersion": 3,
  "created": "2026-10-17T18:40:00Z",
  "generator": "btb-cgeditor 0.1.0",
  "element": { "file": "element.json", "id": "el_ab12cd", "name": "Belka główna", "type": "LOWER_THIRD" },
  "assets": [
    { "path": "assets/3f2a…c1.png", "mime": "image/png", "bytes": 18234, "sha1": "3f2a…c1", "name": "logo.png" }
  ]
}
```

`name` to oryginalna nazwa pliku (informacyjna, trafia do `srcName` warstwy). `_packVersion` rośnie przy zmianach niezgodnych wstecz; czytnik
odrzuca nowsze wersje z komunikatem „zaktualizuj aplikację” (tak jak `CG_PACKAGE_VERSION` w BTB-CG).

## 4. Zachowanie przy eksporcie i imporcie

### BTB CG Editor (Rust)

| Operacja | Działanie |
|---|---|
| **Eksport** (Plik → Eksportuj paczkę elementu…) | wszystkie `src` typu `data:` → pliki w `assets/`; `src` = ścieżka względna; zapisuje `element.json` + `manifest.json`. Adresy `http(s)://` i `/uploads/…` zostają bez zmian; przy opcji „Dołącz zasoby zewnętrzne” edytor pobiera je i też pakuje. |
| **Import** (Plik → Otwórz…, przeciągnięcie `.cgpack`) | rozpakowuje w pamięci, weryfikuje sumy, **osadza** pliki z powrotem jako `data:` (jak przy dodawaniu obrazu z pliku), więc element po otwarciu jest samowystarczalny. |
| **Wyślij do banku/biblioteki** | dla elementu z paczki: `POST /api/upload` dla każdego pliku, `src` → `/uploads/…`, potem `POST /api/elements` (alternatywa: wysłać `data:`, serwer sam zewnętrznizuje). |
| Zapis zwykły (Ctrl+S) | bez zmian: JSON v3 z osadzonymi obrazami. Paczka to osobny eksport, nie format roboczy. |

### BTB CG System (Node, gałąź `TRIO`)

| Operacja | Działanie |
|---|---|
| **Import paczki** | nowy endpoint `POST /api/packs` (i przycisk w panelu/Trio): rozpakowanie do pamięci, weryfikacja sum, zapis plików do `uploads/<sha1>.<ext>` (istniejące pomijane), podmiana `assets/…` → `/uploads/…` w elemencie, następnie zwykłe `EditorIO.parse/prepare/validate/merge` do banku lub biblioteki (aktualizacja po `id` zachowuje stan operatora). |
| **Eksport paczki** | `GET /api/packs/:elementId` (lub akcja w UI): pobiera element, wszystkie `/uploads/…` używane w polach `src`, `url`, CSS/HTML (`url(...)`), pakuje i zwraca `.cgpack`. |
| Zależność | ZIP w Node: `yauzl`/`extract-zip` są w `node_modules`, ale to zależności narzędziowe Electrona; do `dependencies` trzeba dodać świadomie jedną bibliotekę (np. `fflate` – bez natywnych modułów, ten sam kod w przeglądarce i w Node). |
| Współdzielenie kodu | logika paczki (`pack-io.js`) podobnie do `editor-io.js`: jeden moduł dla serwera i przeglądarki; nie duplikować w `app.js`. |

### Zależność po stronie Rust
Crate `zip` (deflate) oraz `sha1` – dwie małe zależności; decyzja i test `cargo build` offline przed wdrożeniem. Alternatywa: własny czytnik/pisarz ZIP (store, bez kompresji) – PNG i tak jest skompresowany, więc rozmiar się prawie nie zmieni; `element.json` można pozostawić nieskompresowany. To ogranicza zależności, ale trzeba obsłużyć deflate przy imporcie paczek z innych narzędzi (np. 7-Zip), więc rekomendacja: crate `zip`.

## 5. Bezpieczeństwo

- **Ścieżki w archiwum** (zip-slip): tylko `manifest.json`, `element.json` i `assets/<sha1>.<ext>`; odrzucamy `..`, ścieżki bezwzględne, nieznane katalogi. Nic nie jest zapisywane na dysk poza `uploads/` (po stronie serwera) i to pod nazwą wyliczoną z hasha, nie z archiwum.
- **Limity**: łączny rozmiar po rozpakowaniu (np. 256 MB), liczba plików (np. 200), rozmiar pliku (zgodny z `multer`: 200 MB; w edytorze 12 MB dla osadzanych obrazów) – ochrona przed bombą dekompresyjną.
- **Typ pliku po zawartości**, nie po rozszerzeniu (jak `media::sniff`); dozwolone obrazy (png, jpg, gif, webp, bmp, svg, avif) i wideo (webm, mp4, ogv) zgodnie z `media-store.js`.
- **SVG** może zawierać skrypty: używany jest wyłącznie jako `<img src>`/tekstura (skrypty się nie uruchamiają); nie wstawiać inline do DOM.
- Kod elementu (HTML/CSS/JS ręczny) i tak jest wykonywany w OBS – paczka od nieznanego nadawcy wymaga zaufania; UI importu pokazuje ostrzeżenie przy elementach z kodem ręcznym.

## 6. Zgodność wsteczna

- Zwykły JSON v3 nadal działa wszędzie – paczka jest dodatkiem.
- Czytnik paczki w edytorze i serwerze akceptuje też `data:` w `src` w `element.json` (paczka „leniwa”).
- Zmiany `_packVersion` tylko niezgodne wstecz; nowe pola opcjonalne dodawane bez podbijania wersji.

## 7. Plan wdrożenia

| Etap | Zakres | Repo | Koszt |
|---|---|---|---|
| 0 | ✅ Dodawanie obrazu z dysku i osadzanie `data:` (`src/media.rs`) | edytor | zrobione |
| 1 | Zatwierdzenie nazwy/rozszerzenia, wyboru bibliotek ZIP/SHA-1 i protokołu endpointów | oba | 0,5 d |
| 2 | Edytor: `pack.rs` – zapis/odczyt paczki (manifest, sumy, limity, anty zip-slip) + testy na paczkach wygenerowanych w teście | edytor | 2 d |
| 3 | Edytor: menu Plik → Eksportuj/Importuj paczkę, otwieranie `.cgpack` (Otwórz, CLI `--open`), tłumaczenia, dokumentacja | edytor | 1 d |
| 4 | Edytor: wysyłka paczki na serwer (`/api/upload` + `/api/elements`) | edytor | 1 d |
| 5 | System: `pack-io.js` + `POST /api/packs` + `GET /api/packs/:id`, UI importu/eksportu (panel i Trio), testy Node | System (`TRIO`) | 2–3 d |
| 6 | Test międzyśrodowiskowy: eksport z edytora → import w systemie → eksport z systemu → otwarcie w edytorze; wynik identyczny (porównanie JSON + sumy plików) | oba | 1 d |
| 7 | Przeciąganie `.cgpack` na okno edytora i na panel systemu; ikona/typ MIME w paczkach `.deb`/AppImage (`packaging/`) | oba | 1 d |

Uwaga organizacyjna: katalog `BTB-CG-System-alpha` ma niezatwierdzone zmiany w drzewie roboczym (m.in. `app.js`); etapy 5–6 trzeba zacząć od
uzgodnienia, na jakim commicie gałęzi `TRIO` pracujemy, żeby niczego nie nadpisać. Kontrakt nie zmienia modelu sceny (`scene-model.js`),
więc `compile_matches_btb_cg_scene_model` nie powinien być dotknięty.

## 8. Otwarte decyzje

1. Nazwa rozszerzenia (`.cgpack` / `.cgel` / `.btbcg`) i MIME.
2. Biblioteka ZIP w Node (`fflate` vs `adm-zip`) i w Rust (`zip` vs własny pisarz).
3. Czy paczka może zawierać wideo (domyślnie tak, ale z limitem i bez osadzania w edytorze).
4. Czy paczka w wersji 2 ma przenosić wiele elementów oraz grupy (zbieżność z `cg_package`).
5. Czy import do serwera ma domyślnie trafiać do biblioteki, czy do banku (propozycja: biblioteka; bank na życzenie).
