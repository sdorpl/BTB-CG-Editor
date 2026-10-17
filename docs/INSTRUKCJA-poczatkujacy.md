# BTB CGEditor – instrukcja dla początkujących

Ta instrukcja prowadzi krok po kroku od uruchomienia programu do gotowej belki na wizji w OBS.
Pełny opis funkcji: [DOKUMENTACJA.md](DOKUMENTACJA.md).

## Najważniejsze pojęcia (w 1 minutę)

- **Element** – jedna gotowa grafika, np. belka z nazwiskiem. Zapisujesz ją jako plik `.json`.
- **Warstwa** – składnik elementu: tło (kształt), tekst, logo (obraz) itd. Warstwy leżą jedna nad drugą.
- **IN / OUT** – animacja wejścia na ekran i wyjścia z ekranu.
- **Stop** – moment, w którym grafika stoi na wizji.
- **Klucz** – punkt na osi czasu zapamiętujący wartość (np. pozycję) w danej chwili; między kluczami program liczy ruch.

## 1. Uruchomienie

Zainstaluj paczkę z `dist/` (`.deb` lub AppImage dla Linuksa) albo uruchom `btb-cgeditor`. Okno ma:
menu u góry, listę warstw po lewej, **kanwę** (podgląd 1920×1080) pośrodku, **panel właściwości** po prawej
i **oś czasu** na dole.

Język: domyślnie polski. Angielski włączysz zmienną `BTB_LANG=en`.

## 2. Pierwsza belka (10 minut)

1. **Plik → Nowy element → Belka (Lower Third)**. Pojawi się gotowy szkic: tło, akcent i tekst.
2. **Zaznacz tekst**: kliknij go na kanwie lub na liście warstw. Dwukliknij tekst na kanwie, aby wpisać własny
   (Enter zatwierdza). Albo zmień go w panelu **Właściwości** → pole *Tekst domyślny*.
3. **Przesuń i zmień rozmiar**: przeciągaj warstwę myszą, rogi zmieniają rozmiar, uchwyt nad ramką obraca.
   Strzałki przesuwają o 1 px, Shift+strzałki o 10 px.
4. **Zmień wygląd**: zaznacz tło → *Wypełnienie* (kolor, gradient), *Zaokrąglenie*, *Cień*.
   Dla tekstu: *Czcionka*, *Rozmiar*, *Kolor*.
5. **Dodaj warstwę**: przyciski nad listą warstw (kształt, tekst, obraz…).
6. **Zapisz**: Ctrl+S. Jeśli się pomylisz – Ctrl+Z cofa.

Wskazówka: pomocne jest przyciąganie (pasek nad kanwą) – warstwy „łapią” krawędzie i środek ramki.
Widok dopasujesz klawiszem **F**, zoom to kółko myszy, przesuwanie widoku – spacja i przeciąganie.

## 2a. Dodawanie logo lub zdjęcia z dysku

1. W panelu warstw kliknij **🖼 Obraz z pliku…** (albo **Plik → Dodaj obraz z pliku…**) i wybierz jeden lub kilka plików
   (PNG, JPEG, WebP, GIF, BMP, SVG).
2. Każdy plik staje się osobną warstwą, wpasowaną w ramkę i wyśrodkowaną; nazwa warstwy to nazwa pliku.
3. Chcesz podmienić obraz w istniejącej warstwie? Zaznacz ją i w zakładce **Właściwości** kliknij **Wybierz plik…**.
4. Plik jest wbudowany w element – nie trzeba go nosić osobno, a belka działa w OBS bez dostępu do Twojego dysku.
   Duże obrazy (powyżej 2 MB) powiększają plik elementu, więc zmniejsz je wcześniej.
5. Tryb obrazu (*Dopasowanie*: wpasuj, wypełnij, rozciągnij…) i zaokrąglenie ustawisz w tej samej zakładce.

## 3. Gdzie ma stać belka na ekranie (Layout)

Kliknij na liście warstw **▣ Element (całość)** i przejdź do zakładki **Element**. W sekcji *Pozycja na wizji*
wybierz *Kotwicę* (np. Lewy dolny) i marginesy. To położenie całej belki w kadrze 1920×1080; warstwy w środku
układasz osobno. Całą belkę możesz też przeciągać po kanwie, gdy jest zaznaczona.

## 4. Wartości zmieniane na antenie

W zakładce **Element** → *Wartości (title / pola)* wpisujesz treści (np. TITLE). Aby tekst warstwy zmieniał się
na antenie, ustaw jego *Pole* (np. `TITLE`). Panel operatora BTB-CG pokazuje potem te pola do edycji.

## 5. Animacja wejścia i wyjścia

1. Zaznacz warstwę, zakładka **Ruch** → przełącznik **Warstwa**.
2. W *Wejście (IN)* wybierz typ (np. *Wjazd*), kierunek, czas, opóźnienie i krzywą.
3. W *Wyjście (OUT)* ustaw to samo dla zejścia.
4. Efekt tekstu (np. *Maszyna do pisania*) wybierz w *Efekt tekstu IN*.
5. Przełącznik **Element** ustawia animację całej belki oraz **pętlę uwagi** (np. pulsowanie na wizji).
6. **Podgląd**: przyciski u góry – **▶ IN**, **▶ OUT**, **⟳ Cykl** (wejście → trzymaj → wyjście), **■ Stop**.

Chcesz, by tło wyszło dopiero po tekście? W *OUT* tła ustaw „Po warstwie” → warstwa tekstu.

## 6. Klucze i ścieżka ruchu (jak w After Effects)

1. Na osi czasu kliknij prawym przyciskiem warstwę → **Dodaj kanał** (np. *Pozycja X*). Dwuklik na torze dodaje klucz.
2. Przeciągaj klucze, aby zmienić czas. Prawy klik na kluczu → interpolacja (Liniowa, Easy Ease, Auto Bézier, Hold).
3. Pole **Stop** na osi ustawia moment zatrzymania; klucze po nim należą do wyjścia.
4. Chcesz zmienić gotowy preset? Kliknij **Presety na klucze** – animacje staną się edytowalnymi kluczami.
5. Przycisk **Wykres** otwiera edytor krzywych: ciągnij uchwyty, by wygładzić lub przyspieszyć ruch.
6. *Ścieżka ruchu* (prawy klik na warstwie) pozwala poprowadzić warstwę po krzywej narysowanej na kanwie.

Uwaga: klucze i ścieżka działają na stronie wyjściowej dla OBS; w innych rendererach mogą nie być widoczne.

## 7. Wyświetlenie w OBS

1. Zapisz element (np. `belka.json`).
2. W terminalu:
   `btb-cgeditor --write-motion-html belka.json output-motion.html`
3. W OBS dodaj źródło **Browser** → *Local file* → `output-motion.html`, rozmiar 1920×1080.
4. Dodaj do adresu parametry: `?obs=1` (pokazuj przy włączeniu źródła, ukrywaj przy wyłączeniu) lub `?auto=1`
   (wejście od razu po załadowaniu).

Alternatywnie wyślij element do serwera BTB-CG: **Plik → Wyślij do banku BTB-CG** (adres serwera wpisz w oknie
*Importuj z BTB-CG…*).

## 8. Praca z istniejącymi elementami

- **Plik → Otwórz…** wczytuje plik `.json`.
- **Plik → Importuj z BTB-CG…** pobiera elementy z serwera (domyślnie `localhost:3000`).
- Element pisany ręcznie kodem pokaże komunikat „Element z kodem ręcznym”. Użyj **Konwertuj kod ręczny na scenę**
  (wymaga Node.js i katalogu BTB-CG) albo **Utwórz scenę**, by edytować go graficznie.

## 9. Ściąga skrótów

| Skrót | Działanie |
|---|---|
| Ctrl+S / Ctrl+O | zapisz / otwórz |
| Ctrl+Z / Ctrl+Y | cofnij / ponów |
| Ctrl+C, V, D | kopiuj, wklej, duplikuj warstwę |
| Ctrl+A | zaznacz wszystko |
| Del | usuń zaznaczone |
| Esc | zatrzymaj odtwarzanie / odznacz |
| F | dopasuj widok |
| Ctrl+P | podgląd HTML w przeglądarce |
| Spacja + przeciąganie | przesuwanie widoku |
| Alt (przy przeciąganiu) | wyłącz przyciąganie |
| Shift (przy przeciąganiu) | blokada osi / proporcji |

## 10. Najczęstsze problemy

| Objaw | Co zrobić |
|---|---|
| „Nie jest obsługiwanym obrazem” przy dodawaniu pliku | użyj PNG, JPEG, WebP, GIF, BMP lub SVG; typ sprawdzany jest po zawartości, nie po rozszerzeniu |
| Warstwa się nie rusza | sprawdź kłódkę 🔒 przy warstwie |
| Nie widać animacji | użyj ▶ IN; sprawdź, czy typ animacji nie jest *Brak* |
| Tło belki nie dopasowuje się do tekstu | zaznacz warstwę tekstu → *Zaawansowane* → *Auto-szerokość*; potem zaznacz tło → *Zaawansowane* → *Dopasuj do tekstu* → wybierz tę warstwę (*Rozciągaj*; dla ikony *Przesuwaj*) |
| Tekst jest ściśnięty | to „squash” – dopasowanie do szerokości; zmień szerokość warstwy lub sekcję *Ściskanie tekstu* |
| „Brak renderera BTB-CG” przy Ctrl+P | zaimportuj coś z serwera albo ustaw `BTB_CG_DIR` na katalog BTB-CG |
| Konwersja nie startuje | zainstaluj Node.js i wskaż poprawny katalog BTB-CG |
| Okno nie startuje / jest wolne | uruchom z `--gpu off` albo `--renderer wgpu` |
| Element wygląda inaczej w OBS niż w edytorze | efekty tekstu i maski w edytorze są przybliżone; wiarygodny jest podgląd Ctrl+P lub strona OBS |
