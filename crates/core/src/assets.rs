//! Anlagenverzeichnis: Geräte und Werkzeuge mit Anschaffungsdatum, Preis und Abschreibung.
//!
//! * `Gwg` – geringwertiges Wirtschaftsgut (bis 800 € netto, bei Kleinunternehmern = Bruttopreis): sofort
//!   abziehbar. Die verknüpfte Ausgabe bleibt als normale Betriebsausgabe in der EÜR.
//! * `Linear` – Abschreibung über die Nutzungsdauer, monatsgenau ab dem Anschaffungsmonat. Die verknüpfte
//!   Ausgabe zählt nicht mehr als Betriebsausgabe, stattdessen die AfA je Jahr (siehe `Db::euer_report`).

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{Error, Result},
};

/// Grenze für geringwertige Wirtschaftsgüter in Cent.
pub const GWG_LIMIT_CENTS: i64 = 80_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssetMethod {
    #[default]
    Gwg,
    Linear,
}

impl AssetMethod {
    pub fn label(self) -> &'static str {
        match self {
            AssetMethod::Gwg => "GWG (sofort abziehbar)",
            AssetMethod::Linear => "Linear abschreiben",
        }
    }

    fn key(self) -> &'static str {
        match self {
            AssetMethod::Gwg => "gwg",
            AssetMethod::Linear => "linear",
        }
    }

    fn from_key(key: &str) -> Self {
        if key == "linear" {
            AssetMethod::Linear
        } else {
            AssetMethod::Gwg
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asset {
    pub id: i64,
    pub name: String,
    pub acquired_at: String,
    pub cost_cents: i64,
    pub method: AssetMethod,
    pub useful_life_years: i64,
    pub expense_id: Option<i64>,
    pub note: String,
}

impl Asset {
    /// AfA des Jahres in Cent (0 bei GWG, vor der Anschaffung und nach Ablauf der Nutzungsdauer).
    pub fn afa_for_year(&self, year: i32) -> i64 {
        if self.method != AssetMethod::Linear || self.useful_life_years <= 0 {
            return 0;
        }
        let (Some(start_year), Some(month)) = (
            date::year_of(&self.acquired_at),
            self.acquired_at.get(5..7).and_then(|m| m.parse::<i64>().ok()),
        ) else {
            return 0;
        };
        if year < start_year {
            return 0;
        }
        // letztes Jahr der Nutzungsdauer: Monate ab Anschaffung gezählt
        let end_year = start_year + ((month - 1 + self.useful_life_years * 12 - 1) / 12) as i32;
        let annual = self.cost_cents as f64 / self.useful_life_years as f64;
        let mut remaining = self.cost_cents;
        for y in start_year..=year.min(end_year) {
            let amount = if y == end_year {
                remaining
            } else {
                let months = if y == start_year { 13 - month } else { 12 };
                ((annual * months as f64 / 12.0).round() as i64).min(remaining)
            };
            if y == year {
                return amount;
            }
            remaining -= amount;
        }
        0
    }

    /// AfA je Jahr von der Anschaffung bis zum Ende der Nutzungsdauer.
    pub fn schedule(&self) -> Vec<(i32, i64)> {
        let Some(start) = date::year_of(&self.acquired_at) else { return Vec::new() };
        (start..=start + self.useful_life_years as i32)
            .map(|y| (y, self.afa_for_year(y)))
            .filter(|(_, cents)| *cents > 0)
            .collect()
    }
}

fn asset_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    let method: String = r.get(4)?;
    Ok(Asset {
        id: r.get(0)?,
        name: r.get(1)?,
        acquired_at: r.get(2)?,
        cost_cents: r.get(3)?,
        method: AssetMethod::from_key(&method),
        useful_life_years: r.get(5)?,
        expense_id: r.get(6)?,
        note: r.get(7)?,
    })
}

impl Db {
    pub fn list_assets(&self) -> Result<Vec<Asset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, acquired_at, cost_cents, method, useful_life_years, expense_id, note
             FROM asset ORDER BY acquired_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], asset_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_asset(&self, a: &mut Asset) -> Result<()> {
        a.name = a.name.trim().to_string();
        if a.name.is_empty() {
            return Err(Error::Invalid("Bezeichnung fehlt".into()));
        }
        if !date::is_valid_iso(&a.acquired_at) {
            return Err(Error::Invalid("Anschaffungsdatum fehlt oder ist ungültig".into()));
        }
        if a.cost_cents <= 0 {
            return Err(Error::Invalid("Anschaffungskosten fehlen".into()));
        }
        if a.method == AssetMethod::Linear && a.useful_life_years < 1 {
            return Err(Error::Invalid("Für die lineare Abschreibung wird die Nutzungsdauer in Jahren gebraucht".into()));
        }
        if a.method == AssetMethod::Gwg {
            a.useful_life_years = 0;
        }
        if a.id == 0 {
            self.conn.execute(
                "INSERT INTO asset (name, acquired_at, cost_cents, method, useful_life_years, expense_id, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![a.name, a.acquired_at, a.cost_cents, a.method.key(), a.useful_life_years, a.expense_id, a.note],
            )?;
            a.id = self.conn.last_insert_rowid();
        } else {
            self.conn.execute(
                "UPDATE asset SET name = ?1, acquired_at = ?2, cost_cents = ?3, method = ?4,
                 useful_life_years = ?5, expense_id = ?6, note = ?7 WHERE id = ?8",
                params![a.name, a.acquired_at, a.cost_cents, a.method.key(), a.useful_life_years, a.expense_id, a.note, a.id],
            )?;
        }
        Ok(())
    }

    pub fn delete_asset(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM asset WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance::Expense;

    fn linear(cost: i64, acquired: &str, years: i64) -> Asset {
        Asset {
            name: "CNC".into(),
            acquired_at: acquired.into(),
            cost_cents: cost,
            method: AssetMethod::Linear,
            useful_life_years: years,
            ..Default::default()
        }
    }

    #[test]
    fn linear_afa_is_monthly_and_adds_up_to_the_cost() {
        let a = linear(120_000, "2025-10-10", 3);
        assert_eq!(a.afa_for_year(2024), 0);
        assert_eq!(a.afa_for_year(2025), 10_000);
        assert_eq!(a.afa_for_year(2026), 40_000);
        assert_eq!(a.afa_for_year(2027), 40_000);
        assert_eq!(a.afa_for_year(2028), 30_000);
        assert_eq!(a.afa_for_year(2029), 0);
        assert_eq!(a.schedule().iter().map(|(_, c)| c).sum::<i64>(), 120_000);

        let jan = linear(90_000, "2026-01-15", 3);
        assert_eq!((jan.afa_for_year(2026), jan.afa_for_year(2028), jan.afa_for_year(2029)), (30_000, 30_000, 0));

        let odd = linear(100_000, "2026-03-01", 3); // rundungsempfindlich
        assert_eq!(odd.schedule().iter().map(|(_, c)| c).sum::<i64>(), 100_000);

        let gwg = Asset { method: AssetMethod::Gwg, ..a };
        assert_eq!(gwg.afa_for_year(2026), 0);
    }

    #[test]
    fn asset_validation_and_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.save_asset(&mut Asset::default()).is_err());
        assert!(db.save_asset(&mut linear(50_000, "2026-01-01", 0)).is_err());
        let mut printer = Asset {
            name: "  Bambu P1S ".into(),
            acquired_at: "2024-04-13".into(),
            cost_cents: 75_700,
            useful_life_years: 5,
            ..Default::default()
        };
        db.save_asset(&mut printer).unwrap();
        assert_eq!(printer.name, "Bambu P1S");
        assert_eq!(printer.useful_life_years, 0, "GWG hat keine Nutzungsdauer");
        assert_eq!(db.list_assets().unwrap(), vec![printer.clone()]);
        db.delete_asset(printer.id).unwrap();
        assert!(db.list_assets().unwrap().is_empty());
    }

    #[test]
    fn linear_asset_replaces_its_purchase_in_the_euer_with_afa() {
        let db = Db::open_in_memory().unwrap();
        let category = db.list_categories().unwrap().into_iter().find(|c| c.name == "Werkzeug & Geräte").unwrap().id;
        let mut buy = Expense {
            spent_at: "2025-10-10".into(),
            description: "CNC-Fräse".into(),
            amount_cents: 120_000,
            category_id: category,
            ..Default::default()
        };
        db.save_expense(&mut buy).unwrap();
        let mut other = Expense { description: "Schrauben".into(), amount_cents: 500, ..buy.clone() };
        other.id = 0;
        db.save_expense(&mut other).unwrap();

        // ohne Verknüpfung: volle Ausgabe
        assert_eq!(db.euer_report(2025).unwrap().expense_total_cents, 120_500);

        let mut asset = Asset { expense_id: Some(buy.id), ..linear(120_000, "2025-10-10", 3) };
        db.save_asset(&mut asset).unwrap();
        let r25 = db.euer_report(2025).unwrap();
        assert_eq!(r25.expense_total_cents, 500 + 10_000);
        assert_eq!(r25.expenses.len(), 1, "Anschaffung steht nicht mehr als Ausgabe in der Liste");
        assert!(r25.by_category.iter().any(|c| c.euer_label == "Absetzung für Abnutzung: CNC" && c.total_cents == 10_000));
        assert_eq!(r25.profit_cents, -(500 + 10_000));
        assert_eq!(r25.missing_receipts, 2, "Beleg der Anschaffung zählt weiter");

        let r26 = db.euer_report(2026).unwrap();
        assert_eq!((r26.expense_total_cents, r26.expenses.len()), (40_000, 0));
        assert_eq!(r26.by_category.len(), 1);
    }
}
