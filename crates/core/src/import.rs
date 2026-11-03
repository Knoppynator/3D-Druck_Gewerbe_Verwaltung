//! Einmaliger Import der bisherigen Tabelle `Ausgaben_Tracking.ods` (Blätter „Ausgaben“ und „Einnahmen“)
//! samt der vorhandenen Rechnungs-PDFs.
//!
//! * Ausgaben werden mit ihrer Kategorie übernommen; „Rechnung: Ja“ wird zu „Beleg liegt vor“
//!   (eine Datei lässt sich nicht automatisch zuordnen).
//! * Rechnungen werden als Altbestand mit einer Sammelposition über den Gesamtbetrag angelegt, das vorhandene
//!   PDF wird kopiert und verknüpft. Als Zahlungsdatum gilt das Rechnungsdatum (wie in der Tabelle).

use std::{collections::HashSet, path::Path};

use std::io::Read;
use rusqlite::params;

use crate::{
    date::{self, parse_de},
    db::Db,
    error::{Error, Result},
    invoicing::recipient_text,
    money::{eur_to_cents, format_eur, parse_eur_cents},
};

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyExpense {
    pub spent_at: String,
    pub qty: i64,
    pub description: String,
    /// Spalte „Typ“ der Tabelle (Verbrauchsmaterial, Werkzeug, Versenden)
    pub kind: String,
    pub amount_cents: i64,
    pub receipt_present: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyInvoice {
    /// Schreibweise `JJJJ-NNNN`
    pub number: String,
    pub issued_at: String,
    pub total_cents: i64,
}

#[derive(Debug, Default)]
pub struct LegacyData {
    pub expenses: Vec<LegacyExpense>,
    pub invoices: Vec<LegacyInvoice>,
    pub warnings: Vec<String>,
}

impl LegacyData {
    pub fn summary(&self) -> String {
        let expenses: i64 = self.expenses.iter().map(|e| e.amount_cents).sum();
        let income: i64 = self.invoices.iter().map(|i| i.total_cents).sum();
        format!(
            "{} Ausgaben (zusammen {}), {} Rechnungen (zusammen {})",
            self.expenses.len(),
            format_eur(expenses),
            self.invoices.len(),
            format_eur(income)
        )
    }
}

#[derive(Debug, Default)]
pub struct ImportReport {
    pub expenses_added: usize,
    pub expenses_skipped: usize,
    pub invoices_added: usize,
    pub invoices_skipped: usize,
    pub pdfs_copied: usize,
    pub warnings: Vec<String>,
}

impl ImportReport {
    pub fn summary(&self) -> String {
        format!(
            "Import: {} Ausgaben neu ({} schon vorhanden), {} Rechnungen neu ({} schon vorhanden), {} PDFs kopiert{}",
            self.expenses_added,
            self.expenses_skipped,
            self.invoices_added,
            self.invoices_skipped,
            self.pdfs_copied,
            if self.warnings.is_empty() { String::new() } else { format!(", {} Hinweis(e)", self.warnings.len()) }
        )
    }
}

/// Zelleninhalt einer ODS-Tabelle.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Empty,
    Text(String),
    Number(f64),
    /// ISO-Datum `JJJJ-MM-TT`
    Date(String),
}

impl Cell {
    fn text(&self) -> String {
        match self {
            Cell::Empty => String::new(),
            Cell::Text(s) => s.trim().to_string(),
            Cell::Number(n) if n.fract() == 0.0 => format!("{}", *n as i64),
            Cell::Number(n) => n.to_string(),
            Cell::Date(d) => d.clone(),
        }
    }

    fn date(&self) -> Option<String> {
        match self {
            Cell::Date(d) => Some(d.clone()),
            Cell::Text(s) => parse_de(s),
            _ => None,
        }
    }

    fn cents(&self) -> Option<i64> {
        match self {
            Cell::Number(n) => Some(eur_to_cents(*n)),
            Cell::Text(s) => parse_eur_cents(s),
            _ => None,
        }
    }
}

const NS_TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const NS_OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";

/// Liest alle Blätter aus dem `content.xml` einer ODS-Datei (Name → Zeilen). Wiederholte Spalten/Zeilen
/// werden ausgeschrieben, leere Zeilen am Ende weggelassen.
fn read_ods_sheets(path: &Path) -> Result<Vec<(String, Vec<Vec<Cell>>)>> {
    let bad = |e: &dyn std::fmt::Display| Error::Invalid(format!("Tabelle konnte nicht gelesen werden: {e}"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|e| bad(&e))?;
    let mut xml = String::new();
    archive
        .by_name("content.xml")
        .map_err(|e| bad(&e))?
        .read_to_string(&mut xml)
        .map_err(|e| bad(&e))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| bad(&e))?;

    let is = |n: &roxmltree::Node, local: &str| {
        n.is_element() && n.tag_name().name() == local && n.tag_name().namespace() == Some(NS_TABLE)
    };
    let repeat = |n: &roxmltree::Node, attr: &str| {
        n.attribute((NS_TABLE, attr)).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1)
    };

    let mut sheets = Vec::new();
    for table in doc.descendants().filter(|n| is(n, "table")) {
        let name = table.attribute((NS_TABLE, "name")).unwrap_or_default().to_string();
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        for row in table.descendants().filter(|n| is(n, "table-row")) {
            let mut cells: Vec<Cell> = Vec::new();
            for cell in row.children().filter(|n| is(n, "table-cell") || is(n, "covered-table-cell")) {
                let value = match cell.attribute((NS_OFFICE, "value-type")) {
                    Some("float" | "currency" | "percentage") => cell
                        .attribute((NS_OFFICE, "value"))
                        .and_then(|v| v.parse::<f64>().ok())
                        .map_or(Cell::Empty, Cell::Number),
                    Some("date") => cell
                        .attribute((NS_OFFICE, "date-value"))
                        .and_then(|v| v.get(0..10))
                        .filter(|v| date::is_valid_iso(v))
                        .map_or(Cell::Empty, |v| Cell::Date(v.to_string())),
                    Some(_) => {
                        let text: Vec<String> = cell
                            .descendants()
                            .filter(|n| n.is_element() && n.tag_name().name() == "p")
                            .map(|p| p.descendants().filter(|t| t.is_text()).filter_map(|t| t.text()).collect::<String>())
                            .collect();
                        Cell::Text(text.join("\n"))
                    }
                    None => Cell::Empty,
                };
                // sehr breite Wiederholungen (leere Restspalten) nicht ausschreiben
                let times = if value == Cell::Empty { repeat(&cell, "number-columns-repeated").min(1) } else { repeat(&cell, "number-columns-repeated").min(64) };
                cells.extend(std::iter::repeat_n(value, times));
            }
            let times = if cells.iter().all(|c| *c == Cell::Empty) { 1 } else { repeat(&row, "number-rows-repeated").min(100) };
            for _ in 0..times {
                rows.push(cells.clone());
            }
        }
        while rows.last().is_some_and(|r| r.iter().all(|c| *c == Cell::Empty)) {
            rows.pop();
        }
        sheets.push((name, rows));
    }
    Ok(sheets)
}

/// Spaltenindex einer Überschrift (ohne Beachtung der Groß-/Kleinschreibung).
fn column(header: &[Cell], name: &str) -> Option<usize> {
    header.iter().position(|c| c.text().eq_ignore_ascii_case(name))
}

fn parse_expenses(rows: &[Vec<Cell>], out: &mut LegacyData) {
    let Some((header_at, header)) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| column(r, "Datum").is_some() && column(r, "Kosten").is_some())
    else {
        out.warnings.push("Blatt „Ausgaben“: Überschriftenzeile (Datum, Kosten …) nicht gefunden".into());
        return;
    };
    let (c_date, c_cost) = (column(header, "Datum").unwrap_or(0), column(header, "Kosten").unwrap_or(0));
    let (c_qty, c_text) = (column(header, "Menge"), column(header, "Bezeichnung"));
    let (c_kind, c_receipt) = (column(header, "Typ"), column(header, "Rechnung"));
    let get = |row: &[Cell], c: Option<usize>| c.and_then(|i| row.get(i)).map(Cell::text).unwrap_or_default();

    for (n, row) in rows.iter().enumerate().skip(header_at + 1) {
        let Some(spent_at) = row.get(c_date).and_then(Cell::date) else { continue };
        let description = get(row, c_text);
        let Some(amount_cents) = row.get(c_cost).and_then(Cell::cents) else {
            out.warnings.push(format!("Ausgaben, Zeile {}: Betrag fehlt ({description}) – übersprungen", n + 1));
            continue;
        };
        if description.is_empty() {
            out.warnings.push(format!("Ausgaben, Zeile {}: Bezeichnung fehlt – übersprungen", n + 1));
            continue;
        }
        let qty = c_qty.and_then(|i| row.get(i)).and_then(|d| d.text().parse::<i64>().ok()).unwrap_or(1);
        out.expenses.push(LegacyExpense {
            spent_at,
            qty: qty.max(0),
            description,
            kind: get(row, c_kind),
            amount_cents,
            receipt_present: get(row, c_receipt).eq_ignore_ascii_case("ja"),
        });
    }
}

fn parse_invoices(rows: &[Vec<Cell>], out: &mut LegacyData) {
    let Some((header_at, header)) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| column(r, "Datum").is_some() && column(r, "Nr").is_some() && column(r, "Preis").is_some())
    else {
        out.warnings.push("Blatt „Einnahmen“: Überschriftenzeile (Datum, Nr, Preis) nicht gefunden".into());
        return;
    };
    let (c_date, c_nr, c_price) = (
        column(header, "Datum").unwrap_or(0),
        column(header, "Nr").unwrap_or(0),
        column(header, "Preis").unwrap_or(0),
    );
    for (n, row) in rows.iter().enumerate().skip(header_at + 1) {
        let Some(issued_at) = row.get(c_date).and_then(Cell::date) else { continue };
        let number = row.get(c_nr).map(Cell::text).unwrap_or_default().replace('_', "-");
        let Some(total_cents) = row.get(c_price).and_then(Cell::cents) else {
            out.warnings.push(format!("Einnahmen, Zeile {}: Betrag fehlt ({number}) – übersprungen", n + 1));
            continue;
        };
        if number.is_empty() {
            out.warnings.push(format!("Einnahmen, Zeile {}: Rechnungsnummer fehlt – übersprungen", n + 1));
            continue;
        }
        out.invoices.push(LegacyInvoice { number, issued_at, total_cents });
    }
}

/// Liest die Tabelle (nur lesend, es wird nichts verändert).
pub fn read_legacy_ods(path: &Path) -> Result<LegacyData> {
    let sheets = read_ods_sheets(path)?;
    let mut data = LegacyData::default();
    match sheets.iter().find(|(name, _)| name == "Ausgaben") {
        Some((_, rows)) => parse_expenses(rows, &mut data),
        None => data.warnings.push("Blatt „Ausgaben“ nicht gefunden".into()),
    }
    match sheets.iter().find(|(name, _)| name == "Einnahmen") {
        Some((_, rows)) => parse_invoices(rows, &mut data),
        None => data.warnings.push("Blatt „Einnahmen“ nicht gefunden".into()),
    }
    Ok(data)
}

/// Kategorie-Name für den „Typ“ der Tabelle.
fn category_for(kind: &str) -> &'static str {
    match kind.to_lowercase().as_str() {
        "verbrauchsmaterial" => "Material & Zukaufteile",
        "werkzeug" => "Werkzeug & Geräte",
        "versenden" => "Versand & Porto",
        _ => "Sonstiges",
    }
}

impl Db {
    /// Übernimmt die gelesenen Daten. Bereits vorhandene Einträge (gleiche Rechnungsnummer bzw. gleiches
    /// Datum, Text, Betrag und Menge) werden übersprungen, der Import lässt sich also wiederholen.
    /// `pdf_dir` ist der Ordner mit `Rechnung_JJJJ_NNNN.pdf`.
    pub fn import_legacy(&self, data: &LegacyData, customer_id: i64, pdf_dir: Option<&Path>) -> Result<ImportReport> {
        let (company, contact, street, zip, city, salutation): (String, String, String, String, String, String) = self
            .conn
            .query_row(
                "SELECT company, contact, street, zip, city, salutation FROM customer WHERE id = ?1",
                [customer_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .map_err(|_| Error::Invalid("Bitte einen Kunden für die Altrechnungen wählen".into()))?;
        let recipient = recipient_text([&company, &contact, &street, &zip, &city]);
        let salutation = if salutation.trim().is_empty() {
            "Sehr geehrte Damen und Herren,".to_string()
        } else {
            salutation
        };
        let payment_days = self.sender()?.payment_days;

        let categories = self.list_categories()?;
        let category_id = |name: &str| categories.iter().find(|c| c.name == name).map(|c| c.id);
        let fallback = category_id("Sonstiges")
            .or_else(|| categories.first().map(|c| c.id))
            .ok_or_else(|| Error::Invalid("Keine Kategorie vorhanden".into()))?;

        let existing: HashSet<(String, String, i64, i64)> = self
            .list_expenses(None)?
            .into_iter()
            .map(|e| (e.expense.spent_at, e.expense.description, e.expense.amount_cents, e.expense.qty))
            .collect();

        let mut report = ImportReport { warnings: data.warnings.clone(), ..Default::default() };
        let mut unknown_kinds = HashSet::new();
        let tx = self.conn.unchecked_transaction()?;

        for e in &data.expenses {
            if existing.contains(&(e.spent_at.clone(), e.description.clone(), e.amount_cents, e.qty)) {
                report.expenses_skipped += 1;
                continue;
            }
            let name = category_for(&e.kind);
            if name == "Sonstiges" && unknown_kinds.insert(e.kind.clone()) {
                report.warnings.push(format!("Unbekannter Typ „{}“ → Kategorie „Sonstiges“", e.kind));
            }
            tx.execute(
                "INSERT INTO expense (spent_at, description, qty, category_id, amount_cents, receipt_present, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Import aus Ausgaben_Tracking.ods')",
                params![
                    e.spent_at,
                    e.description,
                    e.qty,
                    category_id(name).unwrap_or(fallback),
                    e.amount_cents,
                    e.receipt_present
                ],
            )?;
            report.expenses_added += 1;
        }

        for inv in &data.invoices {
            let known: i64 =
                tx.query_row("SELECT COUNT(*) FROM invoice WHERE number = ?1", [&inv.number], |r| r.get(0))?;
            if known > 0 {
                report.invoices_skipped += 1;
                continue;
            }
            let year = inv.number.get(0..4).unwrap_or("0000");
            let suffix = inv.number.get(5..).unwrap_or("");
            let wanted = format!("Rechnung_{year}_{suffix}");
            let mut pdf_path = String::new();
            if let Some(dir) = pdf_dir {
                // gibt es Korrekturfassungen (…_rev2), gilt die letzte
                let mut candidates: Vec<_> = std::fs::read_dir(dir)
                    .map(|rd| rd.filter_map(|f| f.ok()).map(|f| f.path()).collect())
                    .unwrap_or_default();
                candidates.retain(|p| {
                    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
                        && p.file_stem()
                            .and_then(|s| s.to_str())
                            .is_some_and(|s| s == wanted || s.starts_with(&format!("{wanted}_")))
                });
                candidates.sort();
                match candidates.last() {
                    Some(src) => {
                        let rel = format!("rechnungen/{year}/{wanted}.pdf");
                        let target = rel.split('/').fold(self.data_dir().to_path_buf(), |d, part| d.join(part));
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::copy(src, &target)?;
                        pdf_path = rel;
                        report.pdfs_copied += 1;
                    }
                    None => report.warnings.push(format!("Kein PDF zu Rechnung {} gefunden", inv.number)),
                }
            }
            let due_at = self.add_days(&inv.issued_at, payment_days)?;
            tx.execute(
                "INSERT INTO invoice (number, customer_id, issued_at, due_at, recipient, salutation, footer,
                 total_cents, pdf_path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, ?8)",
                params![inv.number, customer_id, inv.issued_at, due_at, recipient, salutation, inv.total_cents, pdf_path],
            )?;
            let invoice_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO invoice_line (invoice_id, pos, description, unit, qty, unit_price_cents, kind)
                 VALUES (?1, 1, 'Altrechnung (Positionen siehe PDF)', 'Pauschal', 1, ?2, 'legacy')",
                params![invoice_id, inv.total_cents],
            )?;
            tx.execute(
                "INSERT INTO payment (invoice_id, paid_at, amount_cents, note)
                 VALUES (?1, ?2, ?3, 'Import: Zahlungsdatum = Rechnungsdatum')",
                params![invoice_id, inv.issued_at, inv.total_cents],
            )?;
            report.invoices_added += 1;
        }

        tx.commit()?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Customer;

    fn resource(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ressourcen").join(rel)
    }

    #[test]
    fn cells_convert_dates_and_amounts() {
        assert_eq!(Cell::Date("2024-01-11".into()).date().as_deref(), Some("2024-01-11"));
        assert_eq!(Cell::Text("11.01.24".into()).date().as_deref(), Some("2024-01-11"));
        assert_eq!(Cell::Number(8.25).cents(), Some(825));
        assert_eq!(Cell::Text("19,99 €".into()).cents(), Some(1999));
        assert_eq!(Cell::Empty.cents(), None);
        assert_eq!(Cell::Number(100.0).text(), "100");
    }

    #[test]
    fn maps_types_to_categories() {
        assert_eq!(category_for("Verbrauchsmaterial"), "Material & Zukaufteile");
        assert_eq!(category_for("werkzeug"), "Werkzeug & Geräte");
        assert_eq!(category_for("Versenden"), "Versand & Porto");
        assert_eq!(category_for("?"), "Sonstiges");
    }

    /// Prüft gegen die echten Dateien aus `ressourcen/`, sofern vorhanden; die erwarteten Summen stehen
    /// in den Jahres-Zwischensummen der Tabelle.
    #[test]
    fn real_spreadsheet_matches_its_own_totals() {
        let ods = resource("Ausgaben/Ausgaben_Tracking.ods");
        if !ods.is_file() {
            return;
        }
        let data = read_legacy_ods(&ods).unwrap();
        assert!(data.warnings.is_empty(), "{:?}", data.warnings);
        assert_eq!(data.invoices.len(), 26);

        let sum = |year: &str| -> (i64, i64) {
            (
                data.expenses.iter().filter(|e| e.spent_at.starts_with(year)).map(|e| e.amount_cents).sum(),
                data.invoices.iter().filter(|i| i.issued_at.starts_with(year)).map(|i| i.total_cents).sum(),
            )
        };
        assert_eq!(sum("2024"), (125_419, 88_096));
        assert_eq!(sum("2025"), (148_305, 440_849));
        assert_eq!(sum("2026"), (90_846, 332_603));

        let db = Db::open_in_memory().unwrap();
        let mut customer = Customer { company: "BluePulse Technologies".into(), ..Default::default() };
        db.save_customer(&mut customer).unwrap();
        let pdfs = resource("Rechnungen");
        let report = db.import_legacy(&data, customer.id, Some(&pdfs)).unwrap();
        assert_eq!((report.expenses_added, report.invoices_added), (data.expenses.len(), 26));
        assert_eq!(report.pdfs_copied, 26);

        let eur = db.euer_report(2025).unwrap();
        assert_eq!((eur.income_total_cents, eur.expense_total_cents), (440_849, 148_305));
        let rows = db.list_invoices().unwrap();
        assert!(rows.iter().all(|r| r.state == crate::invoicing::InvoiceState::Paid));
        assert!(db.invoice_pdf_file(&db.load_invoice(rows[0].id).unwrap()).is_file());
        assert_eq!(db.next_invoice_number(2026).unwrap(), "2026-1012");

        assert!(db.export_euer_pdf(2025).unwrap().is_file());

        let again = db.import_legacy(&data, customer.id, Some(&pdfs)).unwrap();
        assert_eq!((again.expenses_added, again.invoices_added), (0, 0));
        assert_eq!(again.invoices_skipped, 26);
    }
}
