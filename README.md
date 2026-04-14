# mvnx

Maven wrapper skrevet i Rust som gir ryddig og lesbar utdata for flerfoldige prosjekter.

## Funksjoner

- **Ryddig utdata**: Viser kun vesentlig informasjon
- **Reaktor bygge rekkefølge**: Viser rekkefølgen moduler bygges i
- **Framdriftsindikator**: Viser hvilken modul som bygges for tiden
- **Byggesammendrag**: Viser:
  - Reaktor bygge rekkefølge
  - Modulstatus (OK/FAIL) med tidsforbruk
  - Samlet byggestatus og total tidsforbruk
- **Testfeildetaljer**: Viser stacktraces for feilede tester (fra både `.txt` og XML rapporter)
- **XML-basert feilparsing**: Parser Maven Surefire XML-rapporter for detaljerte feilmeldinger
- **Dad jokes**: Valgfri humor under byggingen
- **Clipboard kopiering**: Kopierer stacktraces til utklippstavlen automatisk

## Bruk

Wrapperen tar de samme argumentene som `mvn`:

```bash
# Grunnleggende bygg
mvnx clean install

# Hopp over tester
mvnx clean install -DskipTests

# Spesifikt mål
mvnx clean test

# Egendefinerte innstillinger
mvnx -s ~/settings.xml clean package
```

### Spesielle flagg

- `-h, --help`: Vis hjelpmelding
- `--mvnhelp`: Vis Maven sin hjelpmelding (mvn --help)
- `--clip`: Kopier stacktrace til utklippstavlen når det oppstår nøyaktig en testfeil
- `-j`: Vis dad jokes hvert 30. sekund under byggingen
- `-ji <sekunder>`: Vis dad jokes med egendefinert intervall (impliserer `-j`)

Eksempler:

```bash
mvnx clean install
mvnx --clip test
mvnx -j clean install
mvnx -ji 20 test
mvnx --clip -j package
```

## Utdataeksempel

```
> Building module-a
> Building module-b

================================================================================
BUILD SUMMARY
================================================================================

Reactor Build Order:
  1. module-a
  2. module-b

Module Status:
  OK module-a [2.34s]
  OK module-b [5.67s]

Overall Status: OK SUCCESS
Total Time: 8.01s
Tests: 45 run, 43 passed, 2 failed

================================================================================
TEST FAILURES
================================================================================

[module-b]

--- TestFailureTest.txt ---
java.lang.AssertionError: Expected 42 but got 41
  at TestFailureTest.testSomething(TestFailureTest.java:15)

Stacktrace copied to clipboard.
```

## Testing

Kjør enhetstestene:

```bash
cargo test
```

Testene dekker:
- Parsing av reaktor moduler fra Maven-utdata
- Parsing av modulbyggstartlinjer
- Parsing av testresultatsammendrag
- Filtrering av stacktraces (fjerner rammeverkslinjer, beholder bruker-kode)
- Parsering av Maven Surefire XML-rapporter for feilmeldinger og errorer

## Installasjon

### Bygg fra kildekode

```bash
cargo build --release
# Binær på ./target/release/mvnx
```

### Legg til i PATH

Kopier binæren til en lokasjon i PATH:

```bash
cp target/release/mvnx ~/.local/bin/
# eller
sudo cp target/release/mvnx /usr/local/bin/
```

Bruk det da som:
```bash
mvnx clean install
```

## Hvordan det fungerer

Wrapperen:
1. Starter Maven som en underprosess
2. Fanger og parser dens stdout-utdata
3. Trekker ut nøkkelinformasjon:
   - Reaktor modul rekkefølge
   - Modulbygging framgang
   - Bygje fullføring status og tidsforbruk
   - Testfeilinformasjon
4. Parser Maven Surefire rapporter:
   - Leser `.txt` filer for sammendrag
   - Leser `TEST-*.xml` filer for detaljerte stacktraces og feilmeldinger
   - Filtrerer ut rammeverks-relaterte stacktracelinjer
5. Viser en ryddig sammendrag med feildetaljer
6. Avslutter med Mavens utgangskode

## Krav

- Maven installert og tilgjengelig i PATH
- Et clipboard-verktøy:
  - `wl-copy` (Wayland)
  - `xclip` (X11)
  - `pbcopy` (macOS)

## Begrensninger

- Optimalisert for standard Maven-utdataformat
- Testfeil parsing kan trenge justering basert på ditt testframework
- Krever `mvn` installert og i PATH
