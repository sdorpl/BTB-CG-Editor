# btb-cgeditor: zasady dla agentów i współpracowników

Te zasady obowiązują każdego człowieka i każdego agenta AI. `CLAUDE.md` zawiera instrukcje robocze, ten plik zawiera
reguły, których nie wolno łamać. Gdy oba pliki się różnią, **wygrywa ten plik**.

## 1. Nie wywalaj aplikacji (never crash)

Operator CG pracuje na żywo, a edytor trzyma godziny pracy nad elementami. Uszkodzony plik elementu lub paczki,
zły JSON z serwera BTB-CG, brakujący obraz, zła czcionka, błędne dane klucza/ścieżki albo pełny dysk mają kończyć się
**komunikatem o błędzie**, nigdy awarią ani utratą pracy. Ta reguła ma pierwszeństwo przed nowymi funkcjami.

- **Błędy przez `Result<T, E>`** i propagacja `?` z kontekstem (`map_err`); nie odrzucaj błędu po cichu.
- **Nowy kod nie ma `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`** poza testami.
  Używaj `?`, `ok_or`, `let … else`, `if let` albo `unwrap_or*` tam, gdzie wartość zastępcza jest poprawna
  (nigdy tam, gdzie zepsułaby dokument). Wyjątek: dowodnie nieomylny literał z komentarzem, dlaczego.
- **`unsafe` tylko w granicy WASM** (`btb-motion-wasm`, C ABI): każdy blok z komentarzem `SAFETY`, długości i
  wskaźniki z hosta traktowane jako niezaufane. Gdzie indziej `unsafe` jest zabronione.
- **Każda liczba i indeks pochodzący z pliku, użytkownika lub serwera jest wrogi:** `get()` zamiast `[i]` i
  `[a..b]`; ciąć napisy tylko na granicy znaków; arytmetyka `checked_*`/`saturating_*` dla długości i rozmiarów;
  chronić dzielenie przez zero oraz NaN/inf (`f32` w kluczach, czasach i ścieżkach); ograniczać alokacje
  zależne od wejścia (rozmiar obrazu, liczba kluczy, liczba warstw, rozmiar paczki).
- **Ogranicz rekurencję:** zagnieżdżone elementy i importowane drzewa DOM mogą być cykliczne lub bardzo głębokie.
- **Nie kaskaduj:** zatrutą blokadę (`Mutex`) obsługuj (`unwrap_or_else(PoisonError::into_inner)`), a wynik
  `join` wątku traktuj jako `Result`.
- **Dowód:** każda poprawka awarii ma mały syntetyczny test regresji, który przed poprawką się wywracał.

Testy, `tools/` i skrypty mogą używać `unwrap()`.

**Stan faktyczny:** istniejący kod ma jeszcze użycia `unwrap()`/`expect()` (lista prac: `docs/LUKI.md`, G1).
Nie dodawaj nowych; przy dotykaniu pliku usuwaj te, które leżą na ścieżce danych z zewnątrz. Nie rób masowego
przepisywania poza prośbą.

## 2. Pliki i zasoby: tylko na otwartych licencjach

Dotyczy każdego zasobu: czcionki, ikony, obrazy, presety, szablony, dźwięki, dane wkompilowane (`include_bytes!`,
base64) i pliki w repozytorium (`assets/`, `tests/`, `CG elements converted/`, `packaging/`).

1. **Żadnych zasobów Adobe** w żadnej postaci: ikon, kursorów, czcionek, presetów, szablonów, zrzutów ekranu, ani
   „odrysowanych” kopii ikon After Effects. Ikony rysujemy sami (ogólne konwencje: trójkąt play, nożyczki).
2. **Każdy zasób musi być otwarty:** licencja otwarta (MIT, Apache-2.0, BSD, ISC, zlib, SIL OFL…), domena publiczna/CC0,
   CC BY / CC BY-SA (bez NC/ND) albo wykonany przez autora, który udziela go projektowi na licencji projektu.
   Gdy licencja jest nieznana lub niejasna, zasób nie wchodzi.
3. **Czcionki i grafiki klientów:** elementy telewizyjne z logotypami, znakami towarowymi lub czcionkami komercyjnymi
   (np. `BAHNSCHRIFT.TTF`) nie trafiają do przykładów, testów ani paczek publicznych bez wyraźnego prawa do
   redystrybucji. Rozstrzygnij to z użytkownikiem; do testów używaj czcionek Liberation (SIL OFL).
4. **Zrzuty ekranu** w repozytorium mogą pokazywać tylko btb-cgeditor z danymi, które sami wygenerowaliśmy.
   Zrzuty z `--shot` są tymczasowe i nie są commitowane (wyjątek: wzorce `tests/ui/golden/`).
5. **Materiał referencyjny zostaje lokalny:** `reference/` jest poza gitem i nie jest dystrybuowany.
6. Gdy masz wątpliwość, zostaw zasób i narysuj lub wygeneruj własny.

## 3. Czysty kod (clean-room)

- Zachowanie After Effects poznajemy z publicznej dokumentacji i z używania programu. Nie czytamy ani nie kopiujemy
  niczego z pakietów aplikacji Adobe, ani z ich ekspresji/skryptów/presetów `.ffx`.
- Nie kopiujemy kodu GPL/LGPL/AGPL. Zależności: licencje permisywne (MIT/Apache-2.0/BSD/ISC/Zlib/Unicode/CC0/BSL-1.0).
- **EffectCraft (`reference/effectcraft/`)** jest MIT OR Apache-2.0: wolno adaptować koncepcje i matematykę, przy
  większych fragmentach dopisz pochodzenie w komentarzu przy kodzie. Nie kopiuj hurtem.
- **GSAP i BTB-CG:** `anim.rs`, `playback.rs`, `compile.rs` są portem zachowania z BTB-CG. Trzymaj się licencji GSAP
  (Standard „no charge”) i nie dystrybuuj w paczkach plików, do których nie masz prawa (patrz `vendor/README.md`).

## 4. Zasady inżynierskie

Szczegóły w `CLAUDE.md`: parytet silnika ze wzorcem, silnik bez egui i modelu, czas w `f32` sekundach, `i18n` w obu
językach, dokumentacja w commicie, bramka `make check` przed każdym commitem, commity po polsku i tylko zielone.

Duże pliki binarne (wideo, obrazy testowe) nie trafiają do repozytorium; fixture’y mają być małe i generowane.

## Zobacz też

- [docs/STATUS.md](docs/STATUS.md): bieżący stan, następne zadanie, blokery (czytaj na starcie sesji)
- [docs/LUKI.md](docs/LUKI.md): znane braki uszeregowane wg ważności i strumienie prac; przeczytaj przed wyborem zadania
- [docs/PROPOZYCJE-FUNKCJI.md](docs/PROPOZYCJE-FUNKCJI.md): lista funkcji do wdrożenia i kolejność
- [docs/PLAN-btb-motion.md](docs/PLAN-btb-motion.md): plan i decyzje silnika
- [docs/DOKUMENTACJA.md](docs/DOKUMENTACJA.md), [docs/INSTRUKCJA-poczatkujacy.md](docs/INSTRUKCJA-poczatkujacy.md): dokumentacja użytkowa
