//! Ausgaben mit Belegen, Kategorien und die Jahresauswertung (EÜR). Einnahmen kommen aus den Zahlungen
//! (Zuflussprinzip: maßgeblich ist das Zahlungsdatum).

use std::path::{Path, PathBuf};

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{map_constraint, Error, Result},
    money::format_eur,
};

// ------------------------------------------------------------- Kategorien

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Category {
    pub id: i64,
    pub name: String,
    /// Zeile der Anlage EÜR (Text, vom Benutzer zu pflegen)
    pub euer_line: String,
    pub euer_label: String,
}

// --------------------------------------------------------------- Ausgaben

#[derive(Debug, Clone, PartialEq)]
pub struct Expense {
    pub id: i64,
    pub spent_at: String,
    pub vendor: String,
    pub description: String,
    pub qty: i64,
    pub category_id: i64,
    pub amount_cents: i64,
    /// relativ zum Datenordner; leer = keine Datei verknüpft
    pub receipt_path: String,
    /// Beleg liegt vor (auch ohne verknüpfte Datei)
    pub receipt_present: bool,
    pub shipment_id: Option<i64>,
    pub note: String,
}

impl Default for Expense {
    fn default() -> Self {
        Expense {
            id: 0,
            spent_at: String::new(),
            vendor: String::new(),
            description: String::new(),
            qty: 1,
            category_id: 0,
            amount_cents: 0,
            receipt_path: String::new(),
            receipt_present: false,
            shipment_id: None,
            note: String::new(),
        }
    }
}

impl Expense {
    pub fn has_receipt(&self) -> bool {
        self.receipt_present || !self.receipt_path.is_empty()
    }

    pub fn receipt_label(&self) -> &'static str {
        if !self.receipt_path.is_empty() {
            "Datei"
        } else if self.receipt_present {
            "liegt vor"
        } else {
            "fehlt"
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpenseRow {
    pub expense: Expense,
    pub category: String,
}

// -------------------------------------------------------------------- EÜR

#[derive(Debug, Clone)]
pub struct IncomeRow {
    pub payment_id: i64,
    pub paid_at: String,
    pub invoice_number: String,
    pub customer: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone)]
pub struct CategorySum {
    pub category: String,
    pub euer_line: String,
    pub euer_label: String,
    pub count: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Default)]
pub struct EuerReport {
    pub year: i32,
    pub income: Vec<IncomeRow>,
    pub income_total_cents: i64,
    pub income_line: String,
    pub income_label: String,
    pub expenses: Vec<ExpenseRow>,
    pub by_category: Vec<CategorySum>,
    pub expense_total_cents: i64,
    pub profit_cents: i64,
    /// Ausgaben des Jahres ohne Beleg
    pub missing_receipts: i64,
}

pub const INCOME_LABEL: &str = "Betriebseinnahmen als Kleinunternehmer (§ 19 UStG)";

fn collect<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    Ok(rows.collect::<rusqlite::Result<Vec<T>>>()?)
}

fn expense_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ExpenseRow> {
    Ok(ExpenseRow {
        expense: Expense {
            id: r.get(0)?,
            spent_at: r.get(1)?,
            vendor: r.get(2)?,
            description: r.get(3)?,
            qty: r.get(4)?,
            category_id: r.get(5)?,
            amount_cents: r.get(6)?,
            receipt_path: r.get(7)?,
            receipt_present: r.get(8)?,
            shipment_id: r.get(9)?,
            note: r.get(10)?,
        },
        category: r.get(11)?,
    })
}

const EXPENSE_SELECT: &str = "SELECT e.id, e.spent_at, e.vendor, e.description, e.qty, e.category_id, e.amount_cents,
        e.receipt_path, e.receipt_present, e.shipment_id, e.note, c.name
     FROM expense e JOIN category c ON c.id = e.category_id";

/// Dateiname ohne Pfad und Sonderzeichen.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "beleg".to_string()
    } else {
        cleaned
    }
}

impl Db {
    // ---- Kategorien

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, euer_line, euer_label FROM category ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(Category { id: r.get(0)?, name: r.get(1)?, euer_line: r.get(2)?, euer_label: r.get(3)? })
        })?;
        collect(rows)
    }

    pub fn save_category(&self, c: &mut Category) -> Result<()> {
        c.name = c.name.trim().to_string();
        if c.name.is_empty() {
            return Err(Error::Invalid("Name fehlt".into()));
        }
        let dup = "Eine Kategorie mit diesem Namen existiert bereits";
        if c.id == 0 {
            self.conn
                .execute(
                    "INSERT INTO category (name, euer_line, euer_label) VALUES (?1, ?2, ?3)",
                    params![c.name, c.euer_line.trim(), c.euer_label.trim()],
                )
                .map_err(|e| map_constraint(e, dup))?;
            c.id = self.conn.last_insert_rowid();
        } else {
            self.conn
                .execute(
                    "UPDATE category SET name = ?1, euer_line = ?2, euer_label = ?3 WHERE id = ?4",
                    params![c.name, c.euer_line.trim(), c.euer_label.trim(), c.id],
                )
                .map_err(|e| map_constraint(e, dup))?;
        }
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM category WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Die Kategorie wird noch von Ausgaben verwendet"))?;
        Ok(())
    }

    // ---- Ausgaben

    /// Ausgaben, neueste zuerst; `year` schränkt auf ein Jahr ein.
    pub fn list_expenses(&self, year: Option<i32>) -> Result<Vec<ExpenseRow>> {
        let sql = format!(
            "{EXPENSE_SELECT} WHERE (?1 IS NULL OR substr(e.spent_at, 1, 4) = ?1) ORDER BY e.spent_at DESC, e.id DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([year.map(|y| y.to_string())], expense_from_row)?;
        collect(rows)
    }

    pub fn load_expense(&self, id: i64) -> Result<Expense> {
        let sql = format!("{EXPENSE_SELECT} WHERE e.id = ?1");
        Ok(self.conn.query_row(&sql, [id], expense_from_row)?.expense)
    }

    pub fn save_expense(&self, e: &mut Expense) -> Result<()> {
        if !date::is_valid_iso(&e.spent_at) {
            return Err(Error::Invalid("Datum fehlt oder ist ungültig".into()));
        }
        e.description = e.description.trim().to_string();
        if e.description.is_empty() {
            return Err(Error::Invalid("Bezeichnung fehlt".into()));
        }
        if e.amount_cents == 0 {
            return Err(Error::Invalid("Betrag fehlt".into()));
        }
        if e.category_id == 0 {
            return Err(Error::Invalid("Kategorie fehlt".into()));
        }
        if e.id == 0 {
            self.conn.execute(
                "INSERT INTO expense (spent_at, vendor, description, qty, category_id, amount_cents,
                 receipt_path, receipt_present, shipment_id, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    e.spent_at, e.vendor, e.description, e.qty, e.category_id, e.amount_cents,
                    e.receipt_path, e.receipt_present, e.shipment_id, e.note
                ],
            )?;
            e.id = self.conn.last_insert_rowid();
        } else {
            self.conn.execute(
                "UPDATE expense SET spent_at = ?1, vendor = ?2, description = ?3, qty = ?4, category_id = ?5,
                 amount_cents = ?6, receipt_path = ?7, receipt_present = ?8, shipment_id = ?9, note = ?10
                 WHERE id = ?11",
                params![
                    e.spent_at, e.vendor, e.description, e.qty, e.category_id, e.amount_cents,
                    e.receipt_path, e.receipt_present, e.shipment_id, e.note, e.id
                ],
            )?;
        }
        Ok(())
    }

    /// Löscht die Ausgabe; eine verknüpfte Belegdatei bleibt im Ordner liegen.
    pub fn delete_expense(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM expense WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Lieferungen zur Auswahl beim Verknüpfen von Portokosten (neueste zuerst).
    pub fn shipment_choices(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, c.company, s.packed_at, s.shipped_at
             FROM shipment s JOIN sales_order o ON o.id = s.order_id JOIN customer c ON c.id = o.customer_id
             ORDER BY s.packed_at DESC, s.id DESC LIMIT 100",
        )?;
        let rows = stmt.query_map([], |r| {
            let (id, company, packed, shipped): (i64, String, String, String) =
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
            let when = if shipped.is_empty() { packed } else { shipped };
            Ok((id, format!("{company} · {}", date::format_de(&when))))
        })?;
        collect(rows)
    }

    // ---- Belege

    /// Absoluter Pfad der verknüpften Belegdatei.
    pub fn receipt_file(&self, e: &Expense) -> Option<PathBuf> {
        (!e.receipt_path.is_empty())
            .then(|| e.receipt_path.split('/').fold(self.data_dir().to_path_buf(), |dir, part| dir.join(part)))
    }

    /// Kopiert eine Datei nach `belege/JJJJ/` und verknüpft sie mit der (gespeicherten) Ausgabe.
    pub fn attach_receipt(&self, expense_id: i64, source: &Path) -> Result<String> {
        if !source.is_file() {
            return Err(Error::Invalid(format!("Datei nicht gefunden: {}", source.display())));
        }
        let e = self.load_expense(expense_id)?;
        let original = source.file_name().and_then(|n| n.to_str()).unwrap_or("beleg");
        let year = e.spent_at.get(0..4).unwrap_or("0000");
        let rel = format!("belege/{year}/{}_{}_{}", e.spent_at, e.id, safe_file_name(original));
        let target = rel.split('/').fold(self.data_dir().to_path_buf(), |dir, part| dir.join(part));
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(source, &target)?;
        self.conn.execute(
            "UPDATE expense SET receipt_path = ?1, receipt_present = 1 WHERE id = ?2",
            params![rel, expense_id],
        )?;
        Ok(rel)
    }

    /// Hebt die Verknüpfung auf; die Datei bleibt erhalten.
    pub fn clear_receipt(&self, expense_id: i64) -> Result<()> {
        self.conn.execute("UPDATE expense SET receipt_path = '' WHERE id = ?1", [expense_id])?;
        Ok(())
    }

    // ---- Auswertung

    pub fn euer_report(&self, year: i32) -> Result<EuerReport> {
        let year_text = year.to_string();
        let income = {
            let mut stmt = self.conn.prepare(
                "SELECT p.id, p.paid_at, i.number, c.company, p.amount_cents
                 FROM payment p JOIN invoice i ON i.id = p.invoice_id JOIN customer c ON c.id = i.customer_id
                 WHERE substr(p.paid_at, 1, 4) = ?1 ORDER BY p.paid_at, p.id",
            )?;
            let rows = stmt.query_map([&year_text], |r| {
                Ok(IncomeRow {
                    payment_id: r.get(0)?,
                    paid_at: r.get(1)?,
                    invoice_number: r.get(2)?,
                    customer: r.get(3)?,
                    amount_cents: r.get(4)?,
                })
            })?;
            collect(rows)?
        };
        // Anschaffungen, die linear abgeschrieben werden, zählen nicht als Ausgabe – stattdessen die AfA des Jahres
        let linear_purchases: std::collections::HashSet<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT expense_id FROM asset WHERE method = 'linear' AND expense_id IS NOT NULL")?;
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
            collect(rows)?.into_iter().collect()
        };
        let all_expenses = self.list_expenses(Some(year))?;
        let missing_receipts = all_expenses.iter().filter(|e| !e.expense.has_receipt()).count() as i64;
        let expenses: Vec<ExpenseRow> =
            all_expenses.into_iter().filter(|e| !linear_purchases.contains(&e.expense.id)).collect();
        let mut by_category = {
            let mut stmt = self.conn.prepare(
                "SELECT c.name, c.euer_line, c.euer_label, COUNT(*), SUM(e.amount_cents)
                 FROM expense e JOIN category c ON c.id = e.category_id
                 WHERE substr(e.spent_at, 1, 4) = ?1
                   AND e.id NOT IN (SELECT expense_id FROM asset WHERE method = 'linear' AND expense_id IS NOT NULL)
                 GROUP BY c.id ORDER BY c.euer_line, c.name COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([&year_text], |r| {
                Ok(CategorySum {
                    category: r.get(0)?,
                    euer_line: r.get(1)?,
                    euer_label: r.get(2)?,
                    count: r.get(3)?,
                    total_cents: r.get(4)?,
                })
            })?;
            collect(rows)?
        };
        let mut afa_total = 0;
        for asset in self.list_assets()? {
            let cents = asset.afa_for_year(year);
            if cents > 0 {
                afa_total += cents;
                by_category.push(CategorySum {
                    category: "Abschreibung (AfA)".to_string(),
                    euer_line: String::new(),
                    euer_label: format!("Absetzung für Abnutzung: {}", asset.name),
                    count: 1,
                    total_cents: cents,
                });
            }
        }
        let income_total_cents: i64 = income.iter().map(|i| i.amount_cents).sum();
        let expense_total_cents: i64 = expenses.iter().map(|e| e.expense.amount_cents).sum::<i64>() + afa_total;
        Ok(EuerReport {
            year,
            missing_receipts,
            income_line: self.setting_or_default("euer_income_line")?,
            income_label: INCOME_LABEL.to_string(),
            profit_cents: income_total_cents - expense_total_cents,
            income,
            income_total_cents,
            expenses,
            by_category,
            expense_total_cents,
        })
    }

    /// Jahre, in denen es Einnahmen oder Ausgaben gibt (neueste zuerst), für die Auswahl.
    pub fn finance_years(&self) -> Result<Vec<i32>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT substr(d, 1, 4) FROM (SELECT spent_at AS d FROM expense UNION ALL SELECT paid_at FROM payment)
             ORDER BY 1 DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(collect(rows)?.iter().filter_map(|y| y.parse().ok()).collect())
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_amount(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{},{:02}", abs / 100, abs % 100)
}

impl EuerReport {
    /// CSV (Semikolon, Dezimalkomma) mit Zusammenfassung, Einnahmen und Ausgaben.
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        let mut line = |fields: &[&str]| {
            out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(";"));
            out.push('\n');
        };
        line(&[&format!("Einnahmen-Überschuss-Rechnung {}", self.year)]);
        line(&[]);
        line(&["Zusammenfassung"]);
        line(&["Position", "EÜR-Zeile", "Betrag in EUR"]);
        line(&[&self.income_label, &self.income_line, &csv_amount(self.income_total_cents)]);
        for c in &self.by_category {
            let label = if c.euer_label.is_empty() { c.category.clone() } else { c.euer_label.clone() };
            line(&[&format!("{} ({})", label, c.category), &c.euer_line, &csv_amount(-c.total_cents)]);
        }
        line(&["Summe Betriebsausgaben", "", &csv_amount(self.expense_total_cents)]);
        line(&["Gewinn / Verlust", "", &csv_amount(self.profit_cents)]);
        line(&[]);
        line(&["Einnahmen (nach Zahlungsdatum)"]);
        line(&["Zahlungsdatum", "Rechnung", "Kunde", "Betrag in EUR"]);
        for i in &self.income {
            line(&[&date::format_de(&i.paid_at), &i.invoice_number, &i.customer, &csv_amount(i.amount_cents)]);
        }
        line(&[]);
        line(&["Ausgaben"]);
        line(&["Datum", "Menge", "Bezeichnung", "Lieferant", "Kategorie", "Betrag in EUR", "Beleg"]);
        for e in self.expenses.iter().rev() {
            let x = &e.expense;
            line(&[
                &date::format_de(&x.spent_at),
                &x.qty.to_string(),
                &x.description,
                &x.vendor,
                &e.category,
                &csv_amount(x.amount_cents),
                x.receipt_label(),
            ]);
        }
        format!("\u{feff}{out}")
    }

    /// Kurzfassung für die Statuszeile.
    pub fn summary(&self) -> String {
        format!(
            "{}: Einnahmen {} · Ausgaben {} · Gewinn {}",
            self.year,
            format_eur(self.income_total_cents),
            format_eur(self.expense_total_cents),
            format_eur(self.profit_cents)
        )
    }
}

impl Db {
    /// Schreibt die Auswertung als PDF nach `exporte/EUER_JJJJ.pdf` (Vorlage: `vorlagen/euer.typ`, falls vorhanden).
    pub fn export_euer_pdf(&self, year: i32) -> Result<PathBuf> {
        let report = self.euer_report(year)?;
        let template = std::fs::read_to_string(self.data_dir().join("vorlagen").join("euer.typ"))
            .unwrap_or_else(|_| crate::pdf::EUER_TEMPLATE.to_string());
        let bytes = crate::pdf::render_euer(&report, &self.sender()?, &template)?;
        let path = self.data_dir().join("exporte").join(format!("EUER_{year}.pdf"));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    /// Schreibt die Auswertung als CSV nach `exporte/EUER_JJJJ.csv`.
    pub fn export_euer_csv(&self, year: i32) -> Result<PathBuf> {
        let report = self.euer_report(year)?;
        let path = self.data_dir().join("exporte").join(format!("EUER_{year}.csv"));
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, report.to_csv())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::Customer,
        invoicing::Payment,
        orders::{Order, OrderLine},
    };

    fn category_id(db: &Db, name: &str) -> i64 {
        db.list_categories().unwrap().into_iter().find(|c| c.name == name).unwrap().id
    }

    fn expense(db: &Db, date: &str, desc: &str, cents: i64, cat: &str, receipt: bool) -> Expense {
        let mut e = Expense {
            spent_at: date.into(),
            description: desc.into(),
            amount_cents: cents,
            category_id: category_id(db, cat),
            receipt_present: receipt,
            ..Default::default()
        };
        db.save_expense(&mut e).unwrap();
        e
    }

    #[test]
    fn expense_validation_and_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let mut bad = Expense::default();
        assert!(db.save_expense(&mut bad).is_err());

        let mut e = expense(&db, "2026-04-26", "  Heat Inserts ", 9210, "Material & Zukaufteile", false);
        assert_eq!(e.description, "Heat Inserts");
        e.vendor = "CNC Kitchen".into();
        e.receipt_present = true;
        db.save_expense(&mut e).unwrap();
        assert_eq!(db.load_expense(e.id).unwrap(), e);
        assert_eq!(db.list_expenses(Some(2026)).unwrap().len(), 1);
        assert!(db.list_expenses(Some(2025)).unwrap().is_empty());
        db.delete_expense(e.id).unwrap();
        assert!(db.list_expenses(None).unwrap().is_empty());
    }

    #[test]
    fn categories_cannot_be_deleted_while_used() {
        let db = Db::open_in_memory().unwrap();
        let e = expense(&db, "2026-01-02", "Schrauben", 1776, "Material & Zukaufteile", true);
        assert!(db.delete_category(e.category_id).is_err());
        let mut c = Category { name: "Software".into(), euer_line: "52".into(), ..Default::default() };
        db.save_category(&mut c).unwrap();
        assert!(db.save_category(&mut Category { name: "Software".into(), ..Default::default() }).is_err());
        db.delete_category(c.id).unwrap();
    }

    #[test]
    fn receipt_file_is_copied_into_the_data_folder() {
        let db = Db::open_in_memory().unwrap();
        let e = expense(&db, "2026-04-26", "Inserts", 9210, "Material & Zukaufteile", false);
        let src_dir = db.data_dir().join("quelle");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("Rechnung 24282 (CNC).pdf");
        std::fs::write(&src, b"%PDF-test").unwrap();

        let rel = db.attach_receipt(e.id, &src).unwrap();
        assert_eq!(rel, "belege/2026/2026-04-26_1_Rechnung_24282__CNC_.pdf");
        let loaded = db.load_expense(e.id).unwrap();
        assert!(loaded.receipt_present && loaded.has_receipt());
        let file = db.receipt_file(&loaded).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"%PDF-test");
        assert!(!file.to_string_lossy().contains('/'));

        db.clear_receipt(e.id).unwrap();
        let cleared = db.load_expense(e.id).unwrap();
        assert!(cleared.receipt_path.is_empty() && cleared.receipt_present);
        assert!(file.is_file(), "Datei bleibt erhalten");
        assert!(db.attach_receipt(e.id, &src_dir.join("gibt-es-nicht.pdf")).is_err());
    }

    #[test]
    fn euer_report_uses_payment_dates_and_groups_by_category() {
        let db = Db::open_in_memory().unwrap();
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        let mut o = Order {
            customer_id: c.id,
            received_at: "2025-12-20".into(),
            lines: vec![OrderLine { description: "Set".into(), qty: 10, unit_price_cents: 1300, ..Default::default() }],
            ..Default::default()
        };
        db.save_order(&mut o).unwrap();
        let mut s = db.new_shipment_draft(o.id).unwrap();
        s.shipped_at = "2025-12-30".into();
        db.save_shipment(&mut s).unwrap();
        let inv = db.create_invoice(s.id).unwrap();
        // Rechnung 2025, bezahlt erst 2026 → zählt als Einnahme 2026
        db.add_payment(&mut Payment { invoice_id: inv, paid_at: "2026-01-08".into(), amount_cents: 13_000, ..Default::default() })
            .unwrap();

        expense(&db, "2026-01-20", "Boards", 34_995, "Material & Zukaufteile", true);
        expense(&db, "2026-02-12", "Versand", 619, "Versand & Porto", true);
        expense(&db, "2026-03-11", "Versand", 619, "Versand & Porto", false);
        expense(&db, "2025-12-15", "Akkus", 12_348, "Material & Zukaufteile", false);
        db.set_setting("euer_income_line", "11").unwrap();

        assert_eq!(db.finance_years().unwrap(), vec![2026, 2025]);
        let r26 = db.euer_report(2026).unwrap();
        assert_eq!(r26.income_total_cents, 13_000);
        assert_eq!(r26.income[0].invoice_number.len(), 9);
        assert_eq!(r26.expense_total_cents, 34_995 + 619 + 619);
        assert_eq!(r26.profit_cents, 13_000 - 36_233);
        assert_eq!(r26.missing_receipts, 1);
        assert_eq!(r26.income_line, "11");
        let porto = r26.by_category.iter().find(|c| c.category == "Versand & Porto").unwrap();
        assert_eq!((porto.count, porto.total_cents), (2, 1238));

        let r25 = db.euer_report(2025).unwrap();
        assert_eq!(r25.income_total_cents, 0);
        assert_eq!(r25.expense_total_cents, 12_348);

        let csv = r26.to_csv();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Einnahmen-Überschuss-Rechnung 2026"));
        assert!(csv.contains("Gewinn / Verlust;;-232,33"), "{csv}");
        assert!(csv.contains("Summe Betriebsausgaben;;362,33"), "{csv}");
        assert!(csv.contains("20.01.2026;1;Boards;;Material & Zukaufteile;349,95;liegt vor"));
        let path = db.export_euer_csv(2026).unwrap();
        assert!(path.is_file());
        let pdf = db.export_euer_pdf(2026).unwrap();
        assert!(std::fs::read(pdf).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn csv_helpers() {
        assert_eq!(csv_amount(-5), "-0,05");
        assert_eq!(csv_amount(123456), "1234,56");
        assert_eq!(csv_field("a;b"), "\"a;b\"");
        assert_eq!(csv_field("x\"y"), "\"x\"\"y\"");
    }
}
