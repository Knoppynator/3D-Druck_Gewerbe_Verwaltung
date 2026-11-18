# UI-Überarbeitung – Vorschlag und Umsetzungsplan

Branch `UI-Update`. Entwürfe (Dashboard, Produktion, Bestellungen, Bausteine, Navigation):
<https://claude.ai/artifact/Ggh2nh5EgRAwWhAsnSfiGQ>. Die Zahlen auf den Entwürfen sind Beispielwerte.

## Ziel

Die Seiten sollen übersichtlicher und ruhiger werden: klar getrennte Bereiche, eine Navigation mit Symbolen, Status auf
einen Blick und ein eigenes App-Symbol. Die Bedienung bleibt gleich, es ändert sich nur die Darstellung. Die
Fachlogik in `crates/core` bleibt unberührt.

## Gestaltung

| Thema | Heute | Vorschlag |
|---|---|---|
| Bereiche | Überschriften untereinander, Trennung nur durch Abstand | Jeder Bereich ist eine **Karte**: weiß, 1 px Linie `#e3e2dc`, Ecken 12 px, Innenabstand 16–20 px, auf grauer Seite `#f4f4f1` |
| Seitenkopf | `heading` | Titel 24 px, eine Zeile Kontext darunter („4 Jobs · fertig Fr 02:10“), Hauptaktionen rechts |
| Navigation | Textliste | Symbol + Text, Gruppen (Arbeit, Finanzen, Stammdaten) in kleinen Großbuchstaben, gewählte Seite hellblau hinterlegt, **Zähler/Punkte** (Offen 5, Rechnungen „1 überfällig“, Produktion grün = Druck läuft, Lager gelb = knapp), unten Sicherungsstatus |
| Status | farbiger Text | **Chips**: Punkt + Text auf heller Fläche. Farben nur mit Bedeutung: Blau = in Arbeit, Gelb = wartet/knapp, Orange = Fehldruck, Rot = überfällig/fehlt, Grün = fertig/bezahlt |
| Hinweise | rote/gelbe Zeile | **Hinweisleiste** mit Symbol, Kurztext und Sprung-Link (Info / Warnung / Kritisch) |
| Schrift | egui-Standard | **Inter** (Regular + SemiBold, OFL-Lizenz) – Ziffern in Tabellen gleich breit |
| Akzent | egui-Blau | `#2a78d6`, ein Akzent für Aktionen und Auswahl |

### Dashboard

- **Leitzahl** „Umsatz 2026“ groß (52 px) mit Verlaufslinie der letzten 12 Monate, daneben vier Kacheln: Gewinn (EÜR),
  Forderungen (mit Chip „überfällig“), offene Bestellungen, Warteschlange.
- **Diagramme** nach Dataviz-Regeln:
  - **Säulen:** höchstens 24 px breit, oben 4 px gerundet, feine Gitterlinien, eine Achse.
  - **Beschriftung:** nur Höchstwert und laufender Monat, alles andere zeigt der Tooltip.
  - **Laufender Monat:** heller dargestellt, weil er noch nicht vollständig ist.
- **Materialreichweite** als Messbalken je Posten. Ein Strich markiert die Lieferzeit, die Farbe zeigt den Zustand, ein Chip
  sagt „jetzt bestellen“ oder „bis 25.11.“.
- **Marge je Produkt:**
  - **Selbstkosten-Balken:** ein Balken aus Zukaufteilen, Filament, Druckzeit und Arbeit.
  - **Staffel-Tabelle:** knappe Margen bekommen einen Chip.

### Produktion

- Oben die **Hinweisleiste**, wenn Filament fehlt.
- **„Druckt gerade“-Karte:**
  - **Fortschritt:** Fortschrittsbalken, Start, Restzeit und Ende.
  - **Aktionen:** „Fertig“ und „Fehldruck …“ direkt dort.
- Daneben drei Kennzahl-Kacheln (Jobs, Druckzeit, Filament).
- **Warteschlange:**
  - **Zeitplan:** ein Zeitstrahl mit allen Jobs. Nachts (kein Plattenwechsel) ist er schraffiert, eine rote Linie markiert „jetzt“.
  - **Tabelle:** Farbpunkt, Start → Ende und Status-Chip („wartet bis 07:00“), dazu Symbol-Knöpfe (hoch, runter, starten, entfernen).
- **Bedarf** (Tabelle mit „Fehlt“-Chips) und **Vorschläge** (Karten mit „Einplanen“) nebeneinander.
- „Zuletzt abgeschlossen“ eingeklappt, Fehldruck mit Chip und „2 Teile brauchbar“.

### Listen-/Detailseiten (Bestellungen als Muster für alle Stammdaten)

- **Liste als Karte:**
  - **Kopf:** oben Suche und Filter-Chips (Offen / Abgeschlossen / Alle).
  - **Einträge:** Nummer + Kunde, Summe, Status-Chip, Termin (überfällig rot).
- **Detail:**
  - **Kopf:** Kopfkarte mit Fortschrittsleiste (Eingegangen → … → Bezahlt) und Speichern/Löschen oben rechts.
  - **Formular:** Felder zweispaltig mit Beschriftung über dem Feld, Kalender-Knopf im Datumsfeld.
  - **Leerzustände:** Positionen als Tabelle, Lieferungen als gestrichelte Karte („Noch nichts verpackt“).

### App-Symbol

Blaues abgerundetes Quadrat, Düse oben, drei Schichten darunter („Schicht für Schicht“). Für 32 px und 16 px gibt es eine
vereinfachte Fassung. Es erscheint als Fenster-/Taskleistensymbol und als Symbol der `.exe`, optional auch im Infobereich.

## Umsetzung mit egui/eframe 0.32

Alles geht mit dem vorhandenen Framework. Die einzige neue Abhängigkeit ist optional (Tray, Phase 4).

| Baustein | Umsetzung | Neue Abhängigkeit |
|---|---|---|
| Farben, Abstände, Ecken | Neues Modul `crates/app/src/theme.rs`: `Visuals` (panel/window fill, Widget-Zustände, Auswahl, `CornerRadius`) und `Spacing` (item_spacing, button_padding, interact_size 32 px) einmal beim Start setzen; Farben als Konstanten (`theme::ACCENT` …) | – |
| Schrift | Inter Regular/SemiBold als `include_bytes!` in `FontDefinitions`; eigene Familie `semibold` für Titel und Zahlen (egui kennt kein „fett“ in einer Familie) | – (Schriftdateien, OFL) |
| Symbole | SVG-Pfade (Lucide-Stil, ISC-Lizenz, plus eigene für Drucker/Zukaufteil) in `icons.rs`; beim Start mit **`resvg` 0.47** – schon über Typst im Projekt – zu weißen Masken gerastert (2× für scharfe Darstellung) und als Textur geladen; Anzeige über `egui::Image::tint(farbe)` | – (`resvg` ist bereits gebaut) |
| Karten | `widgets::card(ui, titel, aktionen, inhalt)` mit `egui::Frame::new().fill().stroke().corner_radius(12).inner_margin(16)` | – |
| Chips, Hinweisleiste, Messbalken | eigene kleine Widgets über `ui.painter()` (abgerundetes Rechteck + Punkt + Text) | – |
| Navigation | eigenes Zeilen-Widget: volle Breite, 36 px, Symbol + Text + Zähler, Hover/Auswahl gemalt; Zähler aus einer kleinen Kennzahl-Abfrage beim Seitenwechsel/Speichern (nicht pro Bild) | – |
| Zweispaltige Bereiche | `ui.available_width()` prüfen: breit → `ui.columns(2, …)`, schmal → untereinander | – |
| Diagramme | `bar_chart` überarbeiten: Säulenbreite ≤ 24 px, `CornerRadius` nur oben, Gitter + y-Achse, gezielte Beschriftung, Hover-Hervorhebung | – |
| Zeitstrahl Produktion | Painter: Rechtecke je Job aus `schedule()`, Nachtfenster schraffiert (schräge Linien im Clip-Rechteck), „jetzt“-Linie | – |
| Fenster-/Taskleistensymbol | App-Symbol beim Start mit `resvg` rastern → `egui::IconData` → `ViewportBuilder::with_icon` | – |
| `.exe`-Symbol (Explorer, angeheftet) | `.ico` (PNG-Bilder 16–256 px, einmalig aus der SVG erzeugt) per `build.rs` + Crate `winresource` einbetten | `winresource` (nur Build) |
| Infobereich (Tray) | optional: Crate `tray-icon` – Symbol mit Status („Druck läuft, fertig 16:40“), Klick holt das Fenster nach vorn | `tray-icon` (optional) |

## Phasen

1. **Fundament:**
   - **Grundlagen:** `theme.rs`, Schrift, Symbol-Textur-Atlas.
   - **Widgets:** Karte, Chip, Hinweisleiste, Seitenkopf, Symbol-Knopf, Messbalken.
   - **Rahmen:** neue Navigation mit Zählern, Fenster-/Taskleistensymbol.
   - **Ergebnis:** Alle Seiten sehen sofort ruhiger aus, auch wenn ihr Inhalt noch nicht umgebaut ist.
2. **Dashboard und Produktion** nach den Entwürfen (Leitzahl, Kacheln, Diagramme, Reichweite, Marge; „Druckt gerade“,
   Zeitstrahl, Warteschlange, Bedarf/Vorschläge).
3. **Übrige Seiten** auf Karten, Chips und Seitenkopf umstellen:
   - **Arbeit:** Offen (Kanban-Karten mit Chips), Bestellungen (Liste/Detail wie im Entwurf), Rechnungen, Lager.
   - **Finanzen:** Ausgaben, EÜR, Anlagen.
   - **Rest:** Stammdaten-Formulare, Einstellungen (in Karten gruppiert).
4. **Symbole außerhalb des Fensters:** `.exe`-Symbol per `build.rs`. Optional Tray-Symbol mit Druckstatus.
5. **Optional: Dunkles Design** – dieselben Bausteine mit eigenen, geprüften Dunkel-Farben (Umschalter in den
   Einstellungen).

Jede Phase endet mit `cargo test`, `cargo clippy` und einem Commit auf `UI-Update`. Gemergt wird erst, wenn alles
ausprobiert ist.

**Stand:** Alle fünf Phasen sind umgesetzt (Branch `UI-Update`).

## Entscheidungen

- **Schrift:** Segoe UI (Windows-Systemschrift, zur Laufzeit aus `C:\Windows\Fonts`, Halbfett aus `seguisb.ttf`); fehlt
  sie, nimmt egui seine Standardschrift. Inter ließe sich später als eingebettete Schrift nachrüsten.
- **Infobereich:** echtes Tray-Symbol mit `tray-icon`. Tooltip mit Druckstatus und offenen Punkten, grüner Punkt solange
  gedruckt wird; Linksklick holt das Fenster nach vorn. Kein Menü, kein „in den Infobereich minimieren“.
- **Dunkles Design:** umschaltbar in den Einstellungen (Hell, Dunkel, wie Windows); alle Farben kommen aus einer Palette
  (`theme::p()`), die beim Umschalten wechselt.
- **`.exe`-Symbol:** `assets/druckplaner.ico` (7 Größen, bis 32 px die vereinfachte Fassung) wird per `build.rs` mit
  `winresource` eingebettet. Neu erzeugen nach Änderungen am SVG:
  `DRUCKPLANER_WRITE_ICO=1 cargo test -p druckplaner ico` (der Test prüft sonst, dass die Datei aktuell ist).

## Risiken

- `build.rs` für das `.exe`-Symbol ist ein neues Build-Skript. Die Windows-Anwendungssteuerung hat solche Skripte früher
  zeitweise blockiert, deshalb kommt es erst in Phase 4.
- egui malt jedes Bild neu. Die Symbole werden deshalb einmalig gerastert, und Kennzahlen für die Navigation werden nur
  beim Seitenwechsel geladen.
