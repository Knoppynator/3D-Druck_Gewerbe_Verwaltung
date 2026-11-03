# Druckplaner

Lokale Verwaltungssoftware für 3D-Druck-Serienfertigung. Konzept und Roadmap: [KONZEPT.md](KONZEPT.md).

## Voraussetzungen (Windows)

1. [rustup](https://rustup.rs) installieren (Standard-Toolchain `stable-x86_64-pc-windows-msvc`).
2. Visual Studio Build Tools mit Workload „Desktopentwicklung mit C++“ (wird für den Linker und das eingebettete SQLite benötigt).
3. Neues Terminal öffnen, `cargo --version` prüfen.

## Befehle

```bash
cargo test            # Fachlogik-Tests (crates/core)
cargo run -p druckplaner
cargo build --release # fertige exe: target/release/druckplaner.exe
```

**Windows-Anwendungssteuerung:** Blockiert Windows frisch gebaute Hilfsprogramme (Fehler `os error 4551`, auch als
„can't find crate for …“), hilft es manchmal, das betroffene Paket mit `cargo clean -p <name>` zu entfernen und neu zu
bauen. Beim ersten Release-Build wurden mehrere Pakete tagelang blockiert; nach einigen Tagen und Wiederholungen ohne
Änderungen lief er durch (`target\release\druckplaner.exe`, ca. 56 MB). Die Debug-Version
(`target\debug\druckplaner.exe`, ca. 97 MB) ist ebenfalls ein vollständiges, eigenständiges Programm.
Zum Weitergeben/Installieren genügt es, die `.exe` irgendwohin zu kopieren (Datenordner siehe unten).

Daten liegen in `%APPDATA%\Druckplaner\druckplaner.sqlite`. Mit der Umgebungsvariable `DRUCKPLANER_DATA`
lässt sich ein anderer Ordner wählen (z. B. zum Testen).

## Erste Schritte

1. **Einstellungen:** Absender, Bank und „Letzte bisherige Rechnungsnr.“ eintragen (z. B. `2026-1011`, dann beginnt
   die Software mit `2026-1012`).
2. **Stammdaten:** Kunde, Filament, Druckteile, Zukaufteile (verkaufbare mit Verkaufspreis), Produkt mit
   Staffelpreisen und Aufpreisen.
3. **Bestellung** anlegen → **Lieferung** (Restmengen, Versanddatum, Versandkosten) → **Rechnung erstellen**
   (PDF landet in `<Datenordner>\rechnungen\JJJJ\`) → **Zahlung** unter „Rechnungen“ erfassen.
4. Die Seite **Offen** zeigt, was als Nächstes zu tun ist.

**Wunschtermin:** Beim Produkt lässt sich eine Standard-Lieferzeit nach Stückzahl hinterlegen (wie die Staffelpreise);
die Bestellung schlägt daraus Eingang + Lieferzeit vor (Knopf „Vorschlag“, bei neuen Bestellungen automatisch, bis du
den Termin selbst änderst). Ohne Staffel gilt die Standard-Lieferzeit aus den Einstellungen. Datumsfelder haben einen
Mini-Kalender (📆); Dezimalzahlen werden mit Komma eingegeben.

## Finanzen

- **Ausgaben:** erfassen, Kategorie wählen, Beleg als Datei anhängen („Datei anhängen …“ kopiert sie nach
  `<Datenordner>\belege\JJJJ\`) oder „Beleg liegt vor“ ankreuzen. Portokosten lassen sich einer Lieferung zuordnen.
- **EÜR:** Jahresauswertung (Einnahmen nach Zahlungsdatum, Ausgaben je Kategorie, Gewinn), Export als PDF und CSV nach
  `<Datenordner>\exporte\`. Die EÜR-Zeilen pflegst du unter *Stammdaten › Kategorien* und in den Einstellungen – bitte
  anhand des aktuellen ELSTER-Formulars prüfen.
- **Staffel je Farbe:** Beim Produkt lässt sich wählen, ob Staffelpreise (und Lieferzeit-Staffel) die Gesamtmenge
  oder die Menge je Farbe zählen.
- **Beleg-Vorschau:** Ausgaben mit angehängtem PDF zeigen den Beleg rechts im Fenster.
- **Altdaten:** *Einstellungen › Altdaten importieren*: `ressourcen\Ausgaben\Ausgaben_Tracking.ods` und den Ordner
  `ressourcen\Rechnungen` wählen, vorher den Kunden der Altrechnungen anlegen, „Prüfen“, dann „Importieren“.
  Altrechnungen bekommen eine Sammelposition (Positionen stehen im PDF) und gelten als am Rechnungsdatum bezahlt.

## Lager und Anlagen

- **Lager:** Bestand = Summe der Bewegungen (Filament in g, fertige Druckteile je Farbe, Zukaufteile). Zuerst den
  Anfangsbestand per **Inventur** erfassen. Wird eine Lieferung als versendet gespeichert, bucht das Programm
  Fertigteile und Zukaufteile gemäß Stückliste automatisch aus (und zurück, wenn die Lieferung geändert wird).
- **Wareneingang:** bei der Ausgabe („Wareneingang buchen“) – optional mit Aktualisierung des Einkaufspreises.
- **Mindestbestand/Lieferzeit** stehen bei Filament und Zukaufteilen; unterschrittene Posten und knappe Reichweite
  erscheinen auf der Seite **Offen** unter „Nachbestellen“.
- **Anlagen:** GWG (bis 800 €, bleibt Betriebsausgabe) oder lineare Abschreibung. Bei linearer AfA die Anschaffungs-
  Ausgabe verknüpfen: sie zählt dann nicht mehr voll, die EÜR enthält stattdessen die AfA des Jahres.

## Produktion

1. **Plattenvorlage** (Stammdaten): Druckzeit, Filamentverbrauch und Teile je Platte. „Aus .3mf übernehmen …“ liest eine
   gesliceten Bambu-Studio-Export (Platte slicen, dann „Plattendatei exportieren“ als `.gcode.3mf` bzw. Projekt nach dem
   Slicen speichern) und ordnet Objekte anhand ihres Namens den Druckteilen zu.
2. **Mindestbestand** fertiger Teile je Farbe: im Lager beim Fertigteil-Posten einstellen.
3. **Produktion:** zeigt den Bedarf (offene Bestellungen + Mindestbestand − Lager − Geplantes), schlägt Platten vor,
   führt die Warteschlange und berechnet Start/Ende je Job. Neue Drucke beginnen nur im Zeitfenster für den
   Plattenwechsel (Einstellungen, Standard 07:00–23:00).
4. **Fertig** bucht Teile ins Lager und das Filament ab. **Fehldruck** fragt nach den brauchbaren Teilen je Druckteil
   (kommen ins Lager) und dem Verbrauch, vorbelegt mit der Standardmenge der Platte; „Zurücknehmen“ macht beides rückgängig.

## Datensicherung

- Beim Beenden entsteht automatisch `druckplaner_backup_JJJJ-MM-TT_HHMM.zip` (konsistenter Datenbank-Abzug, Belege,
  Rechnungen, .3mf-Dateien, eigene Vorlagen) – aber nur, wenn sich seit der letzten Sicherung etwas geändert hat. Die
  letzten N bleiben erhalten. Ziel, Anzahl und Ein/Aus stehen in den Einstellungen; ohne Angabe ist es der Unterordner
  `backups` im Datenordner. **Besser ein anderer Datenträger** (USB-Stick, NAS), sonst schützt die Sicherung nicht vor
  einem Plattenausfall. Fehler beim Sichern landen in `backup.log` im Datenordner.
- **Wiederherstellen:** Einstellungen › „Sicherung entpacken …“ entpackt eine ZIP in einen neuen, leeren Ordner (es wird
  nie etwas überschrieben). Danach das Programm mit diesem Ordner starten:
  `$env:DRUCKPLANER_DATA = "<Ordner>"; .\druckplaner.exe` – oder den Inhalt an die Stelle des Datenordners kopieren.

## Dashboard

Startseite mit Kennzahlen (Umsatz, Gewinn nach EÜR, Forderungen, offene Bestellungen, Druck-Warteschlange), Umsatz- und
Absatzdiagramm der letzten 12 Monate mit Hochrechnung aufs Jahr, Materialreichweite (wann nachbestellen) und der Marge je
Produkt. Die Kalkulation folgt `Preiskalkulation_V2`: Zukaufteile + Filament + Druckzeit × Maschinenstundensatz
(+ Arbeitszeit × Stundensatz) gegen die Staffelpreise. Die Sätze stehen in den Einstellungen, der Filamentpreis kommt
aus Stammdaten › Filament. Für importierte Altrechnungen die Stückzahl bei *Rechnungen* nachtragen, sonst ist der
Absatz der Vergangenheit zu niedrig.

Eigene Rechnungsvorlage: [crates/core/templates/rechnung.typ](crates/core/templates/rechnung.typ) nach
`<Datenordner>\vorlagen\rechnung.typ` kopieren und anpassen; „PDF neu erzeugen“ wendet sie an.

## Aufbau

- `crates/core` – Datenbank, Migrationen, Fachlogik (ohne UI)
- `crates/app` – egui-Oberfläche, nur Darstellung und Aufrufe in `core`
