//! Kennzahlen fürs Dashboard: Umsatz und Absatz je Monat mit Hochrechnung, Materialreichweite und
//! Produktkalkulation (Marge) nach dem Vorbild der bisherigen Preiskalkulation.

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{Error, Result},
    money::parse_eur_cents,
    production::PlateTemplate,
    stock::StockKey,
};

// -------------------------------------------------------- Umsatz und Absatz

/// Umsatz (nach Rechnungsdatum, inkl. Stornos) und verkaufte Stück eines Monats.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthSales {
    /// `JJJJ-MM`
    pub month: String,
    pub revenue_cents: i64,
    /// Verkaufte Produkte (Sets); bei Altrechnungen nur, wenn die Stückzahl nachgetragen wurde
    pub units: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Projection {
    pub year: i32,
    /// Verstrichene Monate im laufenden Jahr (inkl. Bruchteil des aktuellen Monats)
    pub months_elapsed: f64,
    pub revenue_ytd_cents: i64,
    pub units_ytd: i64,
    /// Lineare Hochrechnung aufs Jahr; `None`, solange weniger als ein Monat vergangen ist
    pub revenue_projected_cents: Option<i64>,
    pub units_projected: Option<i64>,
    pub revenue_last12_cents: i64,
    pub units_last12: i64,
    /// Altrechnungen, bei denen die Stückzahl noch fehlt
    pub unknown_units_invoices: i64,
}

fn year_month(day: &str) -> Result<(i32, u32)> {
    let y = day.get(0..4).and_then(|y| y.parse().ok());
    let m = day.get(5..7).and_then(|m| m.parse().ok());
    y.zip(m).ok_or_else(|| Error::Invalid(format!("Ungültiges Datum: {day}")))
}

/// Die letzten `n` Monate bis einschließlich des Monats von `today`, älteste zuerst (`JJJJ-MM`).
fn last_months(today: &str, n: usize) -> Result<Vec<String>> {
    let (mut y, mut m) = year_month(today)?;
    let mut months = Vec::new();
    for _ in 0..n {
        months.push(format!("{y:04}-{m:02}"));
        if m == 1 {
            y -= 1;
            m = 12;
        } else {
            m -= 1;
        }
    }
    months.reverse();
    Ok(months)
}

/// Rechnungs-Stückzahl: nachgetragener Wert, sonst Summe der Produktzeilen.
const UNITS_SQL: &str =
    "COALESCE(i.units, (SELECT COALESCE(SUM(l.qty), 0) FROM invoice_line l WHERE l.invoice_id = i.id AND l.kind = 'product'))";

// ------------------------------------------------------------ Reichweite

#[derive(Debug, Clone)]
pub struct ReachRow {
    pub key: StockKey,
    pub name: String,
    pub unit: String,
    pub qty: i64,
    pub min_qty: i64,
    pub consumed_90d: i64,
    pub days_left: i64,
    pub lead_time_days: i64,
    /// Spätester Bestelltermin (`JJJJ-MM-TT`): Reichweite minus Lieferzeit; heute, wenn schon überfällig
    pub order_by: String,
    pub overdue: bool,
}

// ------------------------------------------------------------ Kalkulation

/// Sätze für die Kostenrechnung.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CostRates {
    pub machine_eur_per_h: f64,
    pub labor_eur_per_h: f64,
    pub filament_eur_per_g: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ProductCost {
    pub product_id: i64,
    pub name: String,
    pub purchase_eur: f64,
    pub filament_eur: f64,
    pub machine_eur: f64,
    pub labor_eur: f64,
    pub print_hours: f64,
    pub filament_g: f64,
    /// Staffelpreise (ab Menge, Stückpreis in Cent)
    pub tiers: Vec<(i64, i64)>,
    pub warnings: Vec<String>,
}

impl ProductCost {
    /// Kosten ohne die eigene Arbeitszeit: Zukaufteile, Filament, Maschine.
    pub fn cost_before_labor(&self) -> f64 {
        self.purchase_eur + self.filament_eur + self.machine_eur
    }

    pub fn cost_total(&self) -> f64 {
        self.cost_before_labor() + self.labor_eur
    }

    pub fn profit_before_labor(&self, price_cents: i64) -> f64 {
        price_cents as f64 / 100.0 - self.cost_before_labor()
    }

    pub fn profit_after_labor(&self, price_cents: i64) -> f64 {
        price_cents as f64 / 100.0 - self.cost_total()
    }

    /// Gewinn vor Arbeitszeit in Prozent des Preises.
    pub fn margin_pct(&self, price_cents: i64) -> Option<f64> {
        (price_cents > 0).then(|| self.profit_before_labor(price_cents) / (price_cents as f64 / 100.0) * 100.0)
    }
}

// ------------------------------------------------------------------ Kacheln

#[derive(Debug, Clone, Default)]
pub struct Tiles {
    pub year: i32,
    /// Rechnungsumsatz des Jahres (nach Rechnungsdatum)
    pub revenue_cents: i64,
    /// Einnahmen, Ausgaben, Gewinn nach EÜR (Zahlungsdatum)
    pub income_cents: i64,
    pub expenses_cents: i64,
    pub profit_cents: i64,
    pub receivables_cents: i64,
    pub overdue_cents: i64,
    /// Noch nicht versendete Stück in offenen Bestellungen
    pub open_units: i64,
    pub open_orders: i64,
    /// Voraussichtliches Ende der Druck-Warteschlange (leer = keine Jobs)
    pub queue_ends_at: String,
    pub low_stock: i64,
}

impl Db {
    /// Umsatz und Absatz der letzten `n` Monate (lückenlos, älteste zuerst).
    pub fn monthly_sales(&self, n: usize) -> Result<Vec<MonthSales>> {
        self.monthly_sales_at(&self.today()?, n)
    }

    fn monthly_sales_at(&self, today: &str, n: usize) -> Result<Vec<MonthSales>> {
        let months = last_months(today, n)?;
        let Some(first) = months.first() else { return Ok(Vec::new()) };
        let sql = format!(
            "SELECT substr(i.issued_at, 1, 7), SUM(i.total_cents), SUM({UNITS_SQL})
             FROM invoice i WHERE substr(i.issued_at, 1, 7) >= ?1 GROUP BY 1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([first], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
        let found = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(months
            .into_iter()
            .map(|month| {
                let (revenue_cents, units) =
                    found.iter().find(|(m, _, _)| *m == month).map_or((0, 0), |(_, r, u)| (*r, *u));
                MonthSales { month, revenue_cents, units }
            })
            .collect())
    }

    /// Hochrechnung aufs laufende Jahr: bisheriger Umsatz bzw. Absatz geteilt durch die verstrichenen Monate mal 12.
    pub fn projection(&self) -> Result<Projection> {
        self.projection_at(&self.today()?)
    }

    fn projection_at(&self, today: &str) -> Result<Projection> {
        let (year, month) = year_month(today)?;
        let day: f64 = today.get(8..10).and_then(|d| d.parse().ok()).unwrap_or(1.0);
        let months_elapsed = (f64::from(month) - 1.0) + day / f64::from(date::days_in_month(year, month));

        let sql = format!(
            "SELECT COALESCE(SUM(i.total_cents), 0), COALESCE(SUM({UNITS_SQL}), 0)
             FROM invoice i WHERE substr(i.issued_at, 1, 4) = ?1"
        );
        let (revenue_ytd_cents, units_ytd): (i64, i64) =
            self.conn.query_row(&sql, [year.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let last12 = self.monthly_sales_at(today, 12)?;
        let unknown_units_invoices: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM invoice i WHERE i.units IS NULL
             AND EXISTS (SELECT 1 FROM invoice_line l WHERE l.invoice_id = i.id AND l.kind = 'legacy')",
            [],
            |r| r.get(0),
        )?;
        let project = |ytd: i64| (months_elapsed >= 1.0).then(|| (ytd as f64 / months_elapsed * 12.0).round() as i64);
        Ok(Projection {
            year,
            months_elapsed,
            revenue_ytd_cents,
            units_ytd,
            revenue_projected_cents: project(revenue_ytd_cents),
            units_projected: project(units_ytd),
            revenue_last12_cents: last12.iter().map(|m| m.revenue_cents).sum(),
            units_last12: last12.iter().map(|m| m.units).sum(),
            unknown_units_invoices,
        })
    }

    /// Stückzahl einer Rechnung von Hand festlegen (`None` = aus den Positionen berechnen).
    pub fn set_invoice_units(&self, invoice_id: i64, units: Option<i64>) -> Result<()> {
        if units.is_some_and(|u| u < 0) {
            return Err(Error::Invalid("Die Stückzahl darf nicht negativ sein".into()));
        }
        self.conn.execute("UPDATE invoice SET units = ?1 WHERE id = ?2", params![units, invoice_id])?;
        Ok(())
    }

    pub fn invoice_units(&self, invoice_id: i64) -> Result<Option<i64>> {
        Ok(self.conn.query_row("SELECT units FROM invoice WHERE id = ?1", [invoice_id], |r| r.get(0))?)
    }

    /// Posten mit Verbrauch der letzten 90 Tage: wie lange der Bestand reicht und bis wann bestellt werden muss.
    pub fn reach_rows(&self) -> Result<Vec<ReachRow>> {
        let today = self.today()?;
        let mut rows = Vec::new();
        for l in self.stock_levels()? {
            let Some(days_left) = l.days_left() else { continue };
            let slack = days_left - l.lead_time_days;
            let order_by = if slack > 0 { self.add_days(&today, slack)? } else { today.clone() };
            rows.push(ReachRow {
                key: l.key,
                name: l.name,
                unit: l.unit,
                qty: l.qty,
                min_qty: l.min_qty,
                consumed_90d: l.consumed_90d,
                days_left,
                lead_time_days: l.lead_time_days,
                overdue: slack <= 0,
                order_by,
            });
        }
        rows.sort_by_key(|r| r.days_left - r.lead_time_days);
        Ok(rows)
    }

    /// Sätze aus den Einstellungen; Filamentpreis als Durchschnitt aller Materialien mit hinterlegtem Preis.
    pub fn cost_rates(&self) -> Result<CostRates> {
        let rate = |key: &str| -> Result<f64> {
            Ok(parse_eur_cents(&self.setting_or_default(key)?).map_or(0.0, |c| c as f64 / 100.0))
        };
        let prices: Vec<i64> =
            self.list_materials()?.into_iter().map(|m| m.price_per_kg_cents).filter(|p| *p > 0).collect();
        let per_kg_cents = if prices.is_empty() { 0.0 } else { prices.iter().sum::<i64>() as f64 / prices.len() as f64 };
        Ok(CostRates {
            machine_eur_per_h: rate("machine_rate")?,
            labor_eur_per_h: rate("labor_rate")?,
            filament_eur_per_g: per_kg_cents / 100.0 / 1000.0,
        })
    }

    /// Selbstkosten je Produkt: Zukaufteile, Filament und Druckzeit der Druckteile (über die Plattenvorlage mit den
    /// meisten Exemplaren; Zeit und Gewicht werden gleichmäßig auf alle Stücke der Platte verteilt) sowie
    /// Arbeitszeit. Dazu die Staffelpreise zum Vergleich.
    pub fn product_costing(&self) -> Result<Vec<ProductCost>> {
        let rates = self.cost_rates()?;
        let templates = self.list_plate_templates()?;
        let purchase_parts = self.list_purchase_parts()?;
        let print_parts = self.list_print_parts()?;

        let mut out = Vec::new();
        for product in self.list_products()? {
            let mut cost = ProductCost {
                product_id: product.id,
                name: product.name.clone(),
                labor_eur: product.labor_minutes as f64 / 60.0 * rates.labor_eur_per_h,
                tiers: product.tiers.iter().map(|t| (t.min_qty, t.unit_price_cents)).collect(),
                ..Default::default()
            };
            cost.tiers.sort();
            for line in &product.bom {
                match line.item {
                    crate::catalog::BomItem::PurchasePart(id) => {
                        if let Some(p) = purchase_parts.iter().find(|p| p.id == id) {
                            cost.purchase_eur += p.unit_cost_eur * line.qty as f64;
                        }
                    }
                    crate::catalog::BomItem::PrintPart(id) => {
                        let name = print_parts.iter().find(|p| p.id == id).map_or("?", |p| p.name.as_str());
                        let best: Option<&PlateTemplate> = templates
                            .iter()
                            .filter(|t| t.qty_of(id) > 0)
                            .max_by_key(|t| (t.qty_of(id), -t.print_seconds));
                        let Some(template) = best else {
                            cost.warnings.push(format!("Für „{name}“ gibt es keine Plattenvorlage – Druckzeit und Filament fehlen"));
                            continue;
                        };
                        let pieces: i64 = template.parts.iter().map(|p| p.qty).sum();
                        let share = line.qty as f64 / pieces as f64;
                        cost.print_hours += template.print_seconds as f64 / 3600.0 * share;
                        cost.filament_g += template.filament_g * share;
                    }
                }
            }
            cost.filament_eur = cost.filament_g * rates.filament_eur_per_g;
            cost.machine_eur = cost.print_hours * rates.machine_eur_per_h;
            if cost.filament_g > 0.0 && rates.filament_eur_per_g == 0.0 {
                cost.warnings.push("Kein Filamentpreis hinterlegt (Stammdaten › Filament) – Materialkosten fehlen".into());
            }
            if cost.tiers.is_empty() {
                cost.warnings.push("Keine Staffelpreise gepflegt".into());
            }
            out.push(cost);
        }
        Ok(out)
    }

    /// Kennzahlen für die Kacheln oben.
    pub fn dashboard_tiles(&self) -> Result<Tiles> {
        let today = self.today()?;
        let (year, _) = year_month(&today)?;
        let sql = "SELECT COALESCE(SUM(total_cents), 0) FROM invoice WHERE substr(issued_at, 1, 4) = ?1";
        let revenue_cents: i64 = self.conn.query_row(sql, [year.to_string()], |r| r.get(0))?;
        let euer = self.euer_report(year)?;
        let open = self.open_items()?;
        let queue = self.schedule()?;
        Ok(Tiles {
            year,
            revenue_cents,
            income_cents: euer.income_total_cents,
            expenses_cents: euer.expense_total_cents,
            profit_cents: euer.profit_cents,
            receivables_cents: open.to_collect.iter().map(|i| i.open_cents).sum(),
            overdue_cents: open.to_collect.iter().filter(|i| i.overdue_days > 0).map(|i| i.open_cents).sum(),
            open_units: self.list_order_summaries()?.iter().map(|s| s.remaining_qty()).sum(),
            open_orders: open.orders.len() as i64,
            queue_ends_at: if queue.jobs.is_empty() { String::new() } else { queue.ends_at },
            low_stock: open.low_stock.len() as i64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::{BomItem, BomLine, Customer, Material, PriceTier, PrintPart, Product, PurchasePart},
        production::{PlateTemplate, TemplatePart},
    };

    /// Rechnung ohne PDF direkt in die Datenbank schreiben.
    fn add_invoice(db: &Db, customer: i64, number: &str, issued: &str, total: i64, lines: &[(&str, i64, &str)]) -> i64 {
        db.conn
            .execute(
                "INSERT INTO invoice (number, customer_id, issued_at, due_at, recipient, salutation, total_cents)
                 VALUES (?1, ?2, ?3, ?3, 'x', 'x', ?4)",
                params![number, customer, issued, total],
            )
            .unwrap();
        let id = db.conn.last_insert_rowid();
        for (i, (desc, qty, kind)) in lines.iter().enumerate() {
            db.conn
                .execute(
                    "INSERT INTO invoice_line (invoice_id, pos, description, unit, qty, unit_price_cents, kind)
                     VALUES (?1, ?2, ?3, 'Stück', ?4, 100, ?5)",
                    params![id, i as i64 + 1, desc, qty, kind],
                )
                .unwrap();
        }
        id
    }

    fn customer(db: &Db) -> i64 {
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        c.id
    }

    #[test]
    fn month_series_is_gapless_and_counts_only_product_units() {
        assert_eq!(last_months("2026-02-15", 4).unwrap(), vec!["2025-11", "2025-12", "2026-01", "2026-02"]);
        let db = Db::open_in_memory().unwrap();
        let c = customer(&db);
        add_invoice(&db, c, "2026-1001", "2026-04-10", 20_000, &[("Set\nPETG-black", 10, "product"), ("Lieferkosten", 1, "shipping"), ("Akku", 30, "part")]);
        add_invoice(&db, c, "2026-1002", "2026-04-20", 5_000, &[("Set\nPETG-black", 4, "product")]);
        add_invoice(&db, c, "2026-1003", "2026-06-02", 9_000, &[("Altrechnung (Positionen siehe PDF)", 1, "legacy")]);
        add_invoice(&db, c, "2026-1004", "2026-06-05", -3_000, &[("Set\nPETG-black", -2, "product")]); // Storno

        let months = db.monthly_sales_at("2026-06-16", 4).unwrap();
        assert_eq!(months.iter().map(|m| m.month.as_str()).collect::<Vec<_>>(), vec!["2026-03", "2026-04", "2026-05", "2026-06"]);
        assert_eq!((months[0].revenue_cents, months[0].units), (0, 0));
        assert_eq!((months[1].revenue_cents, months[1].units), (25_000, 14), "Akkus und Lieferkosten zählen nicht als Absatz");
        assert_eq!((months[2].revenue_cents, months[2].units), (0, 0));
        assert_eq!((months[3].revenue_cents, months[3].units), (6_000, -2), "Altrechnung ohne Stückzahl, Storno mindert");

        // Stückzahl der Altrechnung nachtragen
        let legacy: i64 = db.conn.query_row("SELECT id FROM invoice WHERE number = '2026-1003'", [], |r| r.get(0)).unwrap();
        db.set_invoice_units(legacy, Some(8)).unwrap();
        assert_eq!(db.monthly_sales_at("2026-06-16", 4).unwrap()[3].units, 6);
        assert_eq!(db.invoice_units(legacy).unwrap(), Some(8));
        assert!(db.set_invoice_units(legacy, Some(-1)).is_err());
        db.set_invoice_units(legacy, None).unwrap();
        assert_eq!(db.projection_at("2026-06-16").unwrap().unknown_units_invoices, 1);
    }

    #[test]
    fn projection_scales_the_year_to_date() {
        let db = Db::open_in_memory().unwrap();
        let c = customer(&db);
        add_invoice(&db, c, "2025-1001", "2025-12-20", 99_000, &[("Set\nx", 99, "product")]);
        add_invoice(&db, c, "2026-1001", "2026-01-10", 30_000, &[("Set\nx", 20, "product")]);
        add_invoice(&db, c, "2026-1002", "2026-05-20", 30_000, &[("Set\nx", 10, "product")]);

        // 16.06.: 5 Monate + 16/30 = 5,533 Monate verstrichen
        let p = db.projection_at("2026-06-16").unwrap();
        assert!((p.months_elapsed - (5.0 + 16.0 / 30.0)).abs() < 1e-9);
        assert_eq!((p.revenue_ytd_cents, p.units_ytd), (60_000, 30));
        assert_eq!(p.revenue_projected_cents, Some((60_000.0_f64 / (5.0 + 16.0 / 30.0) * 12.0).round() as i64));
        assert_eq!(p.revenue_last12_cents, 99_000 + 60_000);
        assert_eq!(p.units_last12, 99 + 30);

        // erste Januartage: noch keine sinnvolle Hochrechnung
        assert_eq!(db.projection_at("2026-01-15").unwrap().revenue_projected_cents, None);
    }

    #[test]
    fn reach_tells_when_to_order() {
        let db = Db::open_in_memory().unwrap();
        let mut a = Material { kind: "PETG".into(), color: "black".into(), lead_time_days: 10, ..Default::default() };
        let mut b = Material { kind: "PETG".into(), color: "white".into(), lead_time_days: 10, ..Default::default() };
        db.save_material(&mut a).unwrap();
        db.save_material(&mut b).unwrap();
        let mut cup = PurchasePart { name: "Saugnapf".into(), ..Default::default() };
        db.save_purchase_part(&mut cup).unwrap();
        let today = db.today().unwrap();
        let black = StockKey::Filament { material_id: a.id };
        let white = StockKey::Filament { material_id: b.id };
        let cups = StockKey::PurchasePart { purchase_part_id: cup.id };

        // schwarz: 2000 g übrig, 900 g in 90 Tagen verbraucht → 10 g/Tag → 200 Tage; Lieferzeit 10 → in 190 Tagen bestellen
        db.stock_inventory(black, 2900, &today, "").unwrap();
        db.add_stock_move(black, -900, &today, "production", ("print_job", 1), "").unwrap();
        // weiß: 20 g übrig bei gleichem Verbrauch → 2 Tage, Lieferzeit 10 → überfällig
        db.stock_inventory(white, 920, &today, "").unwrap();
        db.add_stock_move(white, -900, &today, "production", ("print_job", 2), "").unwrap();
        // Saugnäpfe: 5 Stück übrig, 90 in 90 Tagen verbraucht → 5 Tage, keine Lieferzeit → in 5 Tagen bestellen
        db.stock_inventory(cups, 95, &today, "").unwrap();
        db.add_stock_move(cups, -90, &today, "shipment", ("shipment", 1), "").unwrap();

        let rows = db.reach_rows().unwrap();
        assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["PETG – white", "Saugnapf", "PETG – black"], "dringendstes zuerst");
        let w = &rows[0];
        assert!(w.overdue && w.order_by == today && w.days_left == 2);
        let c = &rows[1];
        assert!(!c.overdue && c.days_left == 5);
        assert_eq!(c.order_by, db.add_days(&today, 5).unwrap());
        let k = &rows[2];
        assert_eq!((k.days_left, k.lead_time_days, k.overdue), (200, 10, false));
        assert_eq!(k.order_by, db.add_days(&today, 190).unwrap());
    }

    #[test]
    fn costing_follows_the_price_calculation() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("machine_rate", "0,73").unwrap();
        let mut m = Material { kind: "PETG".into(), color: "black".into(), price_per_kg_cents: 2000, ..Default::default() };
        db.save_material(&mut m).unwrap();
        let mut housing = PrintPart { name: "Housing".into(), ..Default::default() };
        let mut sled = PrintPart { name: "Sled".into(), ..Default::default() };
        db.save_print_part(&mut housing).unwrap();
        db.save_print_part(&mut sled).unwrap();
        let mut cup = PurchasePart { name: "Saugnapf".into(), unit_cost_eur: 0.83, ..Default::default() };
        db.save_purchase_part(&mut cup).unwrap();
        let mut product = Product {
            name: "Case Set".into(),
            labor_minutes: 10,
            bom: vec![
                BomLine { item: BomItem::PrintPart(housing.id), qty: 1 },
                BomLine { item: BomItem::PrintPart(sled.id), qty: 1 },
                BomLine { item: BomItem::PurchasePart(cup.id), qty: 4 },
            ],
            tiers: vec![PriceTier { min_qty: 1, unit_price_cents: 2000 }, PriceTier { min_qty: 10, unit_price_cents: 1300 }],
            ..Default::default()
        };
        db.save_product(&mut product).unwrap();
        // Platte: 6 Housing in 1 h mit 60 g → je Stück 10 g und 1/6 h
        let mut plate = PlateTemplate {
            name: "Housing x6".into(),
            print_seconds: 3600,
            filament_g: 60.0,
            parts: vec![TemplatePart { print_part_id: housing.id, qty: 6 }],
            ..Default::default()
        };
        db.save_plate_template(&mut plate).unwrap();

        let rates = db.cost_rates().unwrap();
        assert_eq!((rates.machine_eur_per_h, rates.labor_eur_per_h), (0.73, 65.0));
        assert!((rates.filament_eur_per_g - 0.02).abs() < 1e-12);

        let c = &db.product_costing().unwrap()[0];
        assert!((c.purchase_eur - 3.32).abs() < 1e-9);
        assert!((c.filament_g - 10.0).abs() < 1e-9 && (c.filament_eur - 0.2).abs() < 1e-9);
        assert!((c.print_hours - 1.0 / 6.0).abs() < 1e-9 && (c.machine_eur - 0.73 / 6.0).abs() < 1e-9);
        assert!((c.labor_eur - 65.0 / 6.0).abs() < 1e-9);
        assert_eq!(c.tiers, vec![(1, 2000), (10, 1300)]);
        assert_eq!(c.warnings.len(), 1, "für „Sled“ fehlt die Plattenvorlage: {:?}", c.warnings);
        assert!(c.warnings[0].contains("Sled"));

        assert!((c.cost_before_labor() - (3.32 + 0.2 + 0.73 / 6.0)).abs() < 1e-9);
        assert!((c.profit_before_labor(1300) - (13.0 - c.cost_before_labor())).abs() < 1e-9);
        assert!((c.profit_after_labor(1300) - (13.0 - c.cost_total())).abs() < 1e-9);
        assert!(c.margin_pct(1300).unwrap() > 60.0 && c.margin_pct(0).is_none());
    }

    #[test]
    fn tiles_combine_the_key_figures() {
        let db = Db::open_in_memory().unwrap();
        let t = db.dashboard_tiles().unwrap();
        assert_eq!((t.revenue_cents, t.income_cents, t.receivables_cents, t.open_orders, t.low_stock), (0, 0, 0, 0, 0));
        assert!(t.queue_ends_at.is_empty());
        assert!(t.year >= 2026);
    }
}
