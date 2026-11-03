# Druckplaner – Konzept

Lokale Desktop-Software (Windows, komplett offline) für ein Kleingewerbe, das 3D-gedruckte Teile in
Serie fertigt und verkauft. Ersetzt das bisherige Kopf-Wissen + `Ausgaben_Tracking.ods` +
Word-/ODT-Rechnungen.

## 1. Ausgangslage (aus den Ressourcen)

| Quelle | Erkenntnis |
|---|---|
| `BOM_and_Assambly.pdf` | Produkt = 4 Druckteile (Housing, Sled, Cover, Cap) + Zukaufteile (Saugnäpfe, Inserts M3/M4, Schrauben, O-Ringe, …), Druckhinweise je Teil |
| `Preiskalkulation_V2.ods` | Kosten: Zukaufteile, Druckzeit × Maschinenstundensatz (Abschreibung + Verschleiß + Wartung), Filament €/g, Arbeitszeit, Marge; Staffelpreise 1/5/10/20 Stück |
| `Rechnungen/` | 26 Rechnungen 2024–2026, Nummern `JJJJ-1xxx`, Kleinunternehmer § 19 UStG, fast ausschließlich ein Kunde (BluePulse Technologies); Varianten je Material/Farbe (PETG-black, PETG-CF-black …), Lieferkosten als Position, Weiterverkauf von Kleinteilen; eine Korrektur (`_rev2`) |
| `Ausgaben_Tracking.ods` | Ausgaben mit Datum/Menge/Bezeichnung/Typ (Verbrauchsmaterial, Werkzeug, Versenden)/Kosten/Beleg ja-nein; Einnahmenliste; Jahressummen |
| `Ausgaben/Rechnungen/` | Belege nach Lieferant sortiert (PDF/PNG) |

**Kernprobleme heute:** offene Bestellungen nur im Kopf, Rechnungen vergessen/verspätet,
Filament „nach Gefühl“, keine Verknüpfung Bestellung ↔ Versand ↔ Rechnung ↔ Zahlung, EÜR manuell.

## 2. Anforderungen (aus den Rückfragen)

- **Produkte:** generisch – beliebige Produkte mit Stückliste aus Druckteilen + Zukaufteilen,
  Varianten je Material/Farbe. Zukaufteile auch einzeln verkaufbar.
- **Kunden:** ein Hauptkunde, aber Kundenstamm als einfache Tabelle.
- **Bestelleingang:** per Messenger/Telefon → schnelle manuelle Erfassung (wenige Klicks).
- **Status:** Eingegangen → In Produktion → Nachbearbeitet → Verpackt → Versendet → Rechnung gestellt → Bezahlt.
- **Teillieferungen regelmäßig:** eine Bestellung kann mehrere Lieferungen haben; Rechnung je Lieferung
  (Rechnungsdatum = Liefer-/Leistungsdatum).
- **Versand:** Porto-Ausgabe (DHL) wird an die Lieferung gehängt, Lieferkosten-Position landet automatisch auf der Rechnung.
- **Rechnung:** manuell per Klick erzeugt, PDF im bisherigen Layout, fortlaufend `JJJJ-1xxx` (nächste: 2026-1012),
  danach unveränderlich; Korrektur = Storno + neue Rechnung. Zahlungseingang wird erfasst, Hinweis bei > 14 Tagen offen.
- **Preise:** Preisliste je Produkt mit Mengenstaffeln + Material-Aufpreis, automatisch vorbelegt, überschreibbar.
- **Inventar:** Filament (g je Material/Farbe), fertige Druckteile je Farbe, Zukaufteile, Anlagen (Drucker etc.).
- **Produktion:** 1 Drucker, genau geplant: Druckjobs aus Plattenvorlagen (welche Teile, wie viele, Druckzeit, Gramm),
  Reihenfolge/Zeitplan. Platte immer einfarbig. Bestellungen haben Vorrang, freie Kapazität füllt Mindestbestände auf.
- **Druckdaten:** Import aus Bambu-Studio-`.3mf` (Druckzeit + Filamentgewicht je Platte).
- **Finanzen:** Ausgaben mit verknüpftem Beleg (Datei wird in Belegordner kopiert), Einnahmen automatisch aus bezahlten
  Rechnungen, Jahres-EÜR als Übersicht + Zuordnung zu Zeilen der Anlage EÜR, Export PDF/CSV.
- **Dashboard:** Absatz-/Umsatztrend mit Hochrechnung, Materialreichweite (wann nachbestellen), Marge je Produkt.
- **Altdaten:** vollständiger Import (Ausgaben, 26 Rechnungen inkl. Positionen, Belege).
- **Backup:** automatisch beim Beenden in wählbaren Ordner (USB/NAS), letzte N Stände behalten.
- **Technik:** eigenes Desktop-Fenster, Rust-Kenntnisse vorhanden.

## 3. Fachliche Struktur (Module)

```
 Stammdaten ──► Bestellungen ──► Produktion ──► Lager ──► Lieferung ──► Rechnung ──► Zahlung
 (Produkte,      (Positionen,     (Bedarf,        (Ledger)   (Teilmengen,   (PDF,        (Einnahme
  Varianten,      Status)          Druckjobs,                 Porto)         Nummer)      gebucht)
  Preise,                          Zeitplan)         ▲                                     │
  Kunden)                                            │                                     ▼
                       Ausgaben (Belege) ── Wareneingang ┘            Finanzen / EÜR ◄──────┘
                                                                            │
                                                     Dashboard ◄────────────┘ (liest alles)
```

### 3.1 Stammdaten
- **Material/Filament-Sorte:** Material (PETG, PETG-CF …), Farbe, Hersteller, Preis/kg, Mindestbestand (g), Lieferzeit.
- **Druckteil:** z. B. „Housing“, Druckhinweise.
- **Plattenvorlage:** Name, Liste (Druckteil × Anzahl), Druckzeit, Filamentgewicht – aus `.3mf` importiert, `.3mf` wird abgelegt.
- **Zukaufteil:** z. B. Saugnapf M4 30 mm, Einheit, Einkaufspreis (gleitender Durchschnitt), Mindestbestand, verkaufbar ja/nein.
- **Produkt:** Stückliste (Druckteile + Zukaufteile + Arbeitszeit für Nachbearbeitung/Montage), Varianten je Material/Farbe.
- **Preisliste:** je Produkt Staffeln (ab 1/5/10/20 Stk.) + Aufpreis je Material.
- **Kunde:** Firma, Ansprechpartner, Anrede, Adresse.
- **Einstellungen:** eigene Firmendaten (Adresse, Steuernummer, IBAN …), Rechnungsnummernkreis, Zahlungsziel,
  Maschinenstundensatz-Parameter (wie in der Preiskalkulation), Datenordner, Backup-Ziel.

### 3.2 Bestellungen & Lieferungen
- Bestellung: Kunde, Eingangsdatum, Kanal, Wunschtermin, Notiz, Positionen (Artikel/Variante, Menge, Preis – vorbelegt aus Preisliste).
- Je Position werden Mengen geführt: *bestellt / reserviert (aus Lager) / geliefert*.
- **Lieferung:** wählt Teilmengen offener Positionen, Status *Verpackt → Versendet*, Porto-Ausgabe anhängbar.
  Beim Versenden werden Teile aus dem Lager ausgebucht.
- Der Bestellstatus wird aus Positionen/Lieferungen **abgeleitet** (nicht doppelt gepflegt); „Nachbearbeitet“ ist ein
  Häkchen am Druckjob bzw. Lagerzugang.
- Übersicht „Was ist offen?“: zu produzieren · zu versenden · Rechnung fehlt · Zahlung offen/überfällig.

### 3.3 Produktion
1. **Bedarfsrechnung:** offene Positionen × Stückliste − verfügbarer Lagerbestand (je Druckteil + Farbe).
2. **Mindestbestände:** zusätzlicher Bedarf mit niedrigerer Priorität.
3. **Jobvorschlag:** wählt passende Plattenvorlagen je Farbe (gieriger Algorithmus reicht), prüft Filamentbestand.
4. **Zeitplan:** Jobs sequentiell auf dem Drucker; Plattenwechsel nur in einstellbaren Zeitfenstern
   (z. B. 7–23 Uhr) → realistische Fertigstellungstermine. Reihenfolge per Drag & Drop änderbar.
5. **Job abschließen:** fertig/fehlgeschlagen; bucht Filament ab und Druckteile zu (bei Fehldruck nur Filament ab).

### 3.4 Lager
- Ein **Bewegungsjournal** (Ledger) für alle Lagerartikel: Zugang (Wareneingang, Druckjob), Abgang (Druckjob-Filament,
  Lieferung, Ausschuss), Korrektur (Inventur). Bestand = Summe der Bewegungen → jederzeit nachvollziehbar.
- Wareneingang direkt aus einer Ausgabe heraus: „4 Spulen PETG-black à 1000 g“ → Lager + Einkaufspreis aktualisiert.
- Anlagen (Drucker, Druckplatten …) mit Anschaffungsdatum/-preis, Nutzungsdauer bzw. GWG-Kennzeichen.

### 3.5 Rechnungen
- Erzeugt aus einer Lieferung: Positionen + Lieferkosten, Nummer aus Nummernkreis, PDF aus Vorlage (Layout wie bisher).
- PDF und Datensatz werden eingefroren (keine nachträgliche Änderung); Storno erzeugt Gegenbeleg.
- Zahlung erfassen (Datum, Betrag) → erzeugt automatisch die Einnahme-Buchung.

### 3.6 Finanzen & EÜR
- Buchungen: Einnahme/Ausgabe, Datum (Zahlungsdatum – EÜR folgt dem Zuflussprinzip), Betrag, Kategorie, Beleg, Verweise
  (Rechnung, Lieferung, Wareneingang, Anlage).
- Kategorien (Material, Zukaufteile, Porto, Werkzeug/GWG, AfA, Sonstiges …) mit **Zuordnung zur EÜR-Zeile je Steuerjahr**
  (Tabelle, da sich die Formularzeilen jährlich ändern können).
- Jahresbericht: Summen je Kategorie und je EÜR-Zeile, Gewinn, Belegliste; Export PDF + CSV.

### 3.7 Dashboard
- Kacheln: offene Bestellungen, zu versenden, Rechnungen fehlen, offene Forderungen, Umsatz/Gewinn lfd. Jahr.
- **Absatz/Umsatz:** Stück und € je Monat, gleitender Durchschnitt + lineare Hochrechnung aufs Jahresende.
- **Materialreichweite:** Verbrauch der letzten 90 Tage + reservierter Bedarf → Tage bis leer; „bestellen bis“ = leer − Lieferzeit.
- **Marge je Produkt/Variante:** Kostenrechnung wie `Preiskalkulation_V2` (Material, Druckzeit × Maschinenstundensatz,
  Zukaufteile, Arbeitszeit) vs. Staffelpreis.

## 4. Datenmodell (SQLite, Kern)

```
customer(id, company, contact, salutation, street, zip, city, …)
material(id, kind, color, vendor, price_per_kg_cents, min_stock_g, lead_time_days)
print_part(id, name, notes)
plate_template(id, name, material_kind, print_seconds, filament_g, file_3mf)
plate_template_part(plate_template_id, print_part_id, qty)
purchase_part(id, name, unit, min_stock, sellable)
product(id, name, labor_minutes)
product_bom(product_id, print_part_id | purchase_part_id, qty)
article(id, product_id | purchase_part_id, material_id?, name)        -- verkaufbare Variante
price_tier(product_id, min_qty, unit_price_cents) ; material_surcharge(product_id, material_kind, cents)
stock_item(id, kind[filament|print_part|purchase_part], ref_id, material_id?)
stock_move(id, stock_item_id, qty, ts, reason, ref_table, ref_id)
order(id, customer_id, received_at, channel, due_date, note)
order_line(id, order_id, article_id, qty, unit_price_cents)
print_job(id, plate_template_id, material_id, planned_start, status, finished_at, post_processed)
shipment(id, order_id, status, packed_at, shipped_at, shipping_cost_cents, expense_id?)
shipment_line(shipment_id, order_line_id, qty)
invoice(id, number, shipment_id, issued_at, total_cents, pdf_path, cancelled_by?)
payment(id, invoice_id, paid_at, amount_cents)
booking(id, type[income|expense], date, amount_cents, category_id, text, receipt_path, ref…)
category(id, name, type) ; euer_line(year, category_id, line_no, label)
asset(id, name, acquired_at, cost_cents, useful_life_years, gwg)
setting(key, value)
```
Geldbeträge als ganze Cent (`i64`), Datum als ISO-Text. Datenordner: `druckplaner.sqlite`, `belege/JJJJ/`,
`rechnungen/JJJJ/`, `3mf/`, `backups/`.

## 5. Architektur-Optionen

Allen gemeinsam: **Rust-Kern** mit der gesamten Fachlogik (testbar ohne UI), **SQLite** (`rusqlite`, bundled,
Migrationen per `rusqlite_migration`), **Typst** als eingebettete PDF-Engine für Rechnungen und EÜR-Bericht
(Vorlage ist eine editierbare Textdatei), `zip` + `quick-xml` für den `.3mf`-Import, `calamine` für den einmaligen
ODS-Import. Alles offline, eine `.exe` + Datenordner.

### A) Tauri 2 + Svelte (Empfehlung)
Rust-Kern als Tauri-Backend, Oberfläche als Svelte-App im System-WebView (WebView2 ist auf Windows 11 vorinstalliert).
- **+** Formulare, Tabellen, Drag & Drop (Zeitplan), Charts (z. B. uPlot/Chart.js) sind im Web-Ökosystem trivial;
  PDF-Vorschau direkt im Fenster; kleine Binary (~10–15 MB).
- **−** Zwei Sprachen (Rust + TypeScript/Svelte), Node/npm nur als Build-Werkzeug nötig (zur Laufzeit nicht).

### B) Reines Rust mit egui/eframe
Immediate-Mode-GUI, `egui_extras` (Tabellen, Datepicker), `egui_plot` (Charts), `rfd` (Dateidialoge).
- **+** Eine Sprache, nur `cargo`, sehr schlank, schnelles Iterieren.
- **−** Formularlastige Business-Oberflächen wirken spartanischer und brauchen mehr Handarbeit (Validierung,
  Fokus, Layout); PDF-Vorschau über `typst-render` als Bild.

### C) Tauri + serverseitiges HTML (htmx), ohne JS-Framework
Rust rendert HTML-Fragmente (`maud`/`askama`), `htmx` + eine Chart-Bibliothek als einzige JS-Dateien, vendored.
- **+** Fast alles in Rust, kein npm, trotzdem Web-Oberfläche.
- **−** Unüblichere Kombination in Tauri (eigenes Protokoll/lokaler Server), interaktive Teile (Drag & Drop,
  dynamische Positionslisten) brauchen trotzdem etwas JS.

**Zu Rust:** Ja, direkt mit Rust zu starten ist sinnvoll – bei vorhandenen Kenntnissen gibt es keinen Grund für einen
Prototyp in einer anderen Sprache. Datenmenge und Rechenlast sind winzig; Rust bringt hier v. a. Robustheit,
eine einzige `.exe` ohne Laufzeitumgebung und gute Bibliotheken (Typst, SQLite).

**Entscheidung: Option B – reines Rust mit egui/eframe.**

## 6. Projektstruktur (Option B, Cargo-Workspace)

```
druckplaner/
├─ Cargo.toml              Workspace
├─ crates/core/            Fachlogik + DB (keine UI-Abhängigkeit, Unit-Tests)
│  ├─ migrations/          SQL-Migrationen
│  ├─ templates/           rechnung.typ, euer.typ (Typst)
│  └─ src/{db, catalog, orders, production, inventory, invoicing, finance, dashboard, import}/
└─ crates/app/             eframe-Binary: nur UI-Zustand + Aufrufe in core
   └─ src/views/{dashboard, bestellungen, produktion, lager, finanzen, rechnungen, stammdaten, einstellungen}.rs
```

Migrationen: nummerierte SQL-Dateien in `crates/core/migrations/`, eingelesen von einem eigenen 15-Zeilen-Runner
über `PRAGMA user_version` (statt `rusqlite_migration`). Varianten sind kein eigener `article`-Datensatz, sondern
Produkt × Material (Bestellpositionen verweisen ab M1 auf `product_id` + `material_id`).

Crates (bewusst wenige): `eframe`/`egui` (zunächst auf 0.32 gepinnt), `egui_extras` (Tabellen, Datepicker), `egui_plot`, `rfd`,
`rusqlite` (bundled), `chrono` (ab M1), Typst (`typst` + `typst-pdf`, ggf. `typst-render`
für die Vorschau), `zip` + `quick-xml`, `calamine`, `thiserror`/`anyhow`.

UI-Grundmuster: linke Navigationsleiste mit den Modulen, rechts Tabelle (sortier-/filterbar) + Detail-/Formularbereich;
jede View hält ihren Formularzustand in einem eigenen Struct, Speichern ruft eine `core`-Funktion auf und lädt neu.

## 7. Umsetzungsreihenfolge

| # | Meilenstein | Nutzen | Stand |
|---|---|---|---|
| M0 | Gerüst, DB-Migrationen, Einstellungen, Stammdaten (Produkte, Varianten, Preise, Kunden) | Basis | gebaut |
| M1 | Bestellungen, Lieferungen (Teillieferung), Rechnungs-PDF, Zahlungen, „Was ist offen?“ | löst das größte Problem sofort | gebaut |
| M2 | Ausgaben mit Belegen, Einnahmen aus Zahlungen, EÜR-Bericht, Altdaten-Import | Jahresabschluss 2026 schon mit der Software | gebaut |
| M3 | Lager-Ledger, Wareneingang aus Ausgaben, Mindestbestände, Anlagen | Filament nicht mehr „nach Gefühl“ | gebaut |
| M4 | `.3mf`-Import, Plattenvorlagen, Bedarfsrechnung, Druckjobs, Zeitplan | Produktionsplanung | gebaut |
| M5 | Dashboard mit Projektionen und Margen | Überblick | gebaut |
| M6 | Auto-Backup (ZIP beim Beenden, letzte N), Entpacken, Release-Build als einzelne `.exe` | Alltagstauglichkeit | gebaut |

## 8. Offene Punkte für später
- EÜR-Zeilennummern je Steuerjahr gegen das aktuelle ELSTER-Formular prüfen und in der Zuordnungstabelle pflegen.
- Rechnungsvorlage: bisheriges Layout nachbauen (Logo/Schrift aus `Rechnung_Vorlage.docx`).
- Altrechnungen: Positionen einmalig per Skript aus den PDFs extrahieren, als CSV prüfen, dann importieren.
