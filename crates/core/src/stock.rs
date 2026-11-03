//! Lager als Bewegungsjournal. Posten: Filament je Material (Gramm), fertige Druckteile je Druckteil und
//! Material (Stück), Zukaufteile (Stück). Der Bestand ist die Summe aller Bewegungen und kann
//! rechnerisch negativ werden (z. B. wenn der Anfangsbestand noch nicht per Inventur erfasst ist).

use std::collections::HashMap;

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{Error, Result},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StockKey {
    Filament { material_id: i64 },
    PrintPart { print_part_id: i64, material_id: i64 },
    PurchasePart { purchase_part_id: i64 },
}

type Columns = (&'static str, Option<i64>, Option<i64>, Option<i64>);

impl StockKey {
    fn columns(self) -> Columns {
        match self {
            StockKey::Filament { material_id } => ("filament", Some(material_id), None, None),
            StockKey::PrintPart { print_part_id, material_id } => {
                ("print_part", Some(material_id), Some(print_part_id), None)
            }
            StockKey::PurchasePart { purchase_part_id } => ("purchase_part", None, None, Some(purchase_part_id)),
        }
    }

    fn from_columns(kind: &str, material: Option<i64>, print_part: Option<i64>, purchase_part: Option<i64>) -> Option<Self> {
        match (kind, material, print_part, purchase_part) {
            ("filament", Some(material_id), None, None) => Some(StockKey::Filament { material_id }),
            ("print_part", Some(material_id), Some(print_part_id), None) => {
                Some(StockKey::PrintPart { print_part_id, material_id })
            }
            ("purchase_part", None, None, Some(purchase_part_id)) => Some(StockKey::PurchasePart { purchase_part_id }),
            _ => None,
        }
    }
}

/// Gründe, die der Benutzer selbst bucht und wieder löschen darf.
const MANUAL_REASONS: [&str; 4] = ["receipt", "inventory", "scrap", "manual"];

pub fn reason_label(reason: &str) -> &'static str {
    match reason {
        "receipt" => "Wareneingang",
        "shipment" => "Lieferung",
        "inventory" => "Inventur",
        "scrap" => "Ausschuss",
        "production" => "Druckjob",
        _ => "Korrektur",
    }
}

#[derive(Debug, Clone)]
pub struct StockLevel {
    pub key: StockKey,
    pub name: String,
    pub unit: String,
    pub qty: i64,
    /// Mindestbestand (0 = keiner festgelegt)
    pub min_qty: i64,
    pub lead_time_days: i64,
    /// Verbrauch der letzten 90 Tage (Lieferungen, Druckjobs)
    pub consumed_90d: i64,
}

impl StockLevel {
    pub fn below_min(&self) -> bool {
        self.min_qty > 0 && self.qty < self.min_qty
    }

    /// Reichweite in Tagen beim Verbrauch der letzten 90 Tage; `None` ohne Verbrauch.
    pub fn days_left(&self) -> Option<i64> {
        (self.consumed_90d > 0).then(|| self.qty.max(0) * 90 / self.consumed_90d)
    }

    /// Bestand reicht (beim aktuellen Verbrauch) nicht mehr bis zur möglichen Lieferung.
    pub fn reorder_now(&self) -> bool {
        self.days_left().is_some_and(|d| d <= self.lead_time_days) && self.consumed_90d > 0
    }
}

#[derive(Debug, Clone)]
pub struct StockMove {
    pub id: i64,
    pub key: StockKey,
    pub qty: i64,
    pub moved_at: String,
    pub reason: String,
    pub ref_type: String,
    pub ref_id: i64,
    pub note: String,
}

impl StockMove {
    /// Vom Benutzer gebucht (und damit löschbar) im Gegensatz zu automatischen Bewegungen.
    pub fn is_manual(&self) -> bool {
        MANUAL_REASONS.contains(&self.reason.as_str())
    }
}

fn move_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<StockMove>> {
    let kind: String = r.get(1)?;
    let key = StockKey::from_columns(&kind, r.get(2)?, r.get(3)?, r.get(4)?);
    let (id, qty, moved_at, reason) = (r.get(0)?, r.get(5)?, r.get(6)?, r.get(7)?);
    let (ref_type, ref_id, note) = (r.get(8)?, r.get(9)?, r.get(10)?);
    Ok(key.map(|key| StockMove { id, key, qty, moved_at, reason, ref_type, ref_id, note }))
}

const MOVE_SELECT: &str = "SELECT id, kind, material_id, print_part_id, purchase_part_id, qty, moved_at, reason, \
     ref_type, ref_id, note FROM stock_move";

impl Db {
    /// Name und Einheit eines Postens.
    pub fn stock_label(&self, key: StockKey) -> Result<(String, String)> {
        Ok(match key {
            StockKey::Filament { material_id } => {
                let (kind, color): (String, String) = self.conn.query_row(
                    "SELECT kind, color FROM material WHERE id = ?1",
                    [material_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                (format!("{kind} – {color}"), "g".to_string())
            }
            StockKey::PrintPart { print_part_id, material_id } => {
                let name: String =
                    self.conn.query_row("SELECT name FROM print_part WHERE id = ?1", [print_part_id], |r| r.get(0))?;
                let (kind, color): (String, String) = self.conn.query_row(
                    "SELECT kind, color FROM material WHERE id = ?1",
                    [material_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                (format!("{name} – {kind} {color}"), "Stk".to_string())
            }
            StockKey::PurchasePart { purchase_part_id } => self.conn.query_row(
                "SELECT name, unit FROM purchase_part WHERE id = ?1",
                [purchase_part_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
        })
    }

    pub fn stock_qty(&self, key: StockKey) -> Result<i64> {
        let (kind, m, pp, bp) = key.columns();
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(qty), 0) FROM stock_move
             WHERE kind = ?1 AND material_id IS ?2 AND print_part_id IS ?3 AND purchase_part_id IS ?4",
            params![kind, m, pp, bp],
            |r| r.get(0),
        )?)
    }

    /// Alle Posten mit Bestand: jedes Filament und jedes Zukaufteil sowie Fertigteile, für die es Bewegungen gibt.
    pub fn stock_levels(&self) -> Result<Vec<StockLevel>> {
        let mut sums: HashMap<StockKey, (i64, i64)> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT kind, material_id, print_part_id, purchase_part_id, SUM(qty),
                   COALESCE(SUM(CASE WHEN qty < 0 AND reason IN ('shipment', 'production')
                                      AND moved_at >= date('now', 'localtime', '-90 days') THEN -qty ELSE 0 END), 0)
                 FROM stock_move GROUP BY kind, material_id, print_part_id, purchase_part_id",
            )?;
            let rows = stmt.query_map([], |r| {
                let kind: String = r.get(0)?;
                Ok((StockKey::from_columns(&kind, r.get(1)?, r.get(2)?, r.get(3)?), r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
            })?;
            for row in rows {
                if let (Some(key), qty, consumed) = row? {
                    sums.insert(key, (qty, consumed));
                }
            }
        }
        let level = |key: StockKey, name: String, unit: String, min_qty: i64, lead: i64, sums: &HashMap<StockKey, (i64, i64)>| {
            let (qty, consumed_90d) = sums.get(&key).copied().unwrap_or((0, 0));
            StockLevel { key, name, unit, qty, min_qty, lead_time_days: lead, consumed_90d }
        };

        let mut levels = Vec::new();
        for m in self.list_materials()? {
            levels.push(level(
                StockKey::Filament { material_id: m.id },
                m.label(),
                "g".into(),
                m.min_stock_g,
                m.lead_time_days,
                &sums,
            ));
        }
        let mins = self.finished_mins()?;
        let mut finished: Vec<StockKey> = sums
            .keys()
            .copied()
            .chain(mins.keys().copied())
            .filter(|k| matches!(k, StockKey::PrintPart { .. }))
            .collect();
        finished.sort();
        finished.dedup();
        for key in finished {
            let (name, unit) = self.stock_label(key)?;
            levels.push(level(key, name, unit, mins.get(&key).copied().unwrap_or(0), 0, &sums));
        }
        for p in self.list_purchase_parts()? {
            levels.push(level(
                StockKey::PurchasePart { purchase_part_id: p.id },
                p.name.clone(),
                p.unit.clone(),
                p.min_stock,
                0,
                &sums,
            ));
        }
        Ok(levels)
    }

    /// Gewünschte Mindestbestände fertiger Druckteile je Farbe.
    pub fn finished_mins(&self) -> Result<HashMap<StockKey, i64>> {
        let mut stmt = self.conn.prepare("SELECT print_part_id, material_id, min_qty FROM finished_min")?;
        let rows = stmt.query_map([], |r| {
            Ok((StockKey::PrintPart { print_part_id: r.get(0)?, material_id: r.get(1)? }, r.get::<_, i64>(2)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
    }

    /// Setzt den Mindestbestand eines Fertigteil-Postens (0 = keiner).
    pub fn set_finished_min(&self, key: StockKey, min_qty: i64) -> Result<()> {
        let StockKey::PrintPart { print_part_id, material_id } = key else {
            return Err(Error::Invalid("Nur für Fertigteile".into()));
        };
        if min_qty <= 0 {
            self.conn.execute(
                "DELETE FROM finished_min WHERE print_part_id = ?1 AND material_id = ?2",
                params![print_part_id, material_id],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO finished_min (print_part_id, material_id, min_qty) VALUES (?1, ?2, ?3)
                 ON CONFLICT(print_part_id, material_id) DO UPDATE SET min_qty = excluded.min_qty",
                params![print_part_id, material_id, min_qty],
            )?;
        }
        Ok(())
    }

    /// Posten unter Mindestbestand oder mit absehbar knappem Bestand (Reichweite ≤ Lieferzeit).
    pub fn stock_alerts(&self) -> Result<Vec<StockLevel>> {
        Ok(self.stock_levels()?.into_iter().filter(|l| l.below_min() || l.reorder_now()).collect())
    }

    /// Neueste Bewegungen eines Postens.
    pub fn stock_moves(&self, key: StockKey, limit: usize) -> Result<Vec<StockMove>> {
        let (kind, m, pp, bp) = key.columns();
        let sql = format!(
            "{MOVE_SELECT} WHERE kind = ?1 AND material_id IS ?2 AND print_part_id IS ?3 AND purchase_part_id IS ?4
             ORDER BY moved_at DESC, id DESC LIMIT ?5"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![kind, m, pp, bp, limit as i64], move_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.extend(row?);
        }
        Ok(out)
    }

    /// Bewegungen, die von einem Datensatz ausgelöst wurden (z. B. Wareneingang zu einer Ausgabe).
    pub fn stock_moves_for_ref(&self, ref_type: &str, ref_id: i64) -> Result<Vec<StockMove>> {
        let sql = format!("{MOVE_SELECT} WHERE ref_type = ?1 AND ref_id = ?2 ORDER BY id");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![ref_type, ref_id], move_from_row)?;
        let mut out = Vec::new();
        for row in rows {
            out.extend(row?);
        }
        Ok(out)
    }

    pub fn add_stock_move(
        &self,
        key: StockKey,
        qty: i64,
        moved_at: &str,
        reason: &str,
        reference: (&str, i64),
        note: &str,
    ) -> Result<i64> {
        if qty == 0 {
            return Err(Error::Invalid("Menge fehlt".into()));
        }
        if !date::is_valid_iso(moved_at) {
            return Err(Error::Invalid("Datum fehlt oder ist ungültig".into()));
        }
        let (kind, m, pp, bp) = key.columns();
        self.conn.execute(
            "INSERT INTO stock_move (kind, material_id, print_part_id, purchase_part_id, qty, moved_at, reason,
             ref_type, ref_id, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![kind, m, pp, bp, qty, moved_at, reason, reference.0, reference.1, note],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Setzt den Bestand per Inventur auf `counted`; gebucht wird die Differenz. Gibt sie zurück.
    pub fn stock_inventory(&self, key: StockKey, counted: i64, moved_at: &str, note: &str) -> Result<i64> {
        if counted < 0 {
            return Err(Error::Invalid("Gezählte Menge darf nicht negativ sein".into()));
        }
        let delta = counted - self.stock_qty(key)?;
        if delta != 0 {
            self.add_stock_move(key, delta, moved_at, "inventory", ("", 0), note)?;
        }
        Ok(delta)
    }

    /// Löscht eine vom Benutzer gebuchte Bewegung; automatische (Lieferung, Druckjob) nicht.
    pub fn delete_stock_move(&self, id: i64) -> Result<()> {
        let reason: String =
            self.conn.query_row("SELECT reason FROM stock_move WHERE id = ?1", [id], |r| r.get(0))?;
        if !MANUAL_REASONS.contains(&reason.as_str()) {
            return Err(Error::Invalid(
                "Diese Bewegung entsteht automatisch (Lieferung/Druckjob) und wird dort geändert".into(),
            ));
        }
        self.conn.execute("DELETE FROM stock_move WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Bucht einen Wareneingang zu einer Ausgabe (Filament in Gramm oder Zukaufteil in Stück). Mit
    /// `update_price` wird der Einkaufspreis des Postens aus dem Betrag der Ausgabe neu berechnet.
    pub fn book_receipt(&self, expense_id: i64, key: StockKey, qty: i64, update_price: bool) -> Result<()> {
        if qty <= 0 {
            return Err(Error::Invalid("Menge muss größer als 0 sein".into()));
        }
        if matches!(key, StockKey::PrintPart { .. }) {
            return Err(Error::Invalid("Fertigteile werden nicht eingekauft, sondern gedruckt".into()));
        }
        let expense = self.load_expense(expense_id)?;
        let tx = self.conn.unchecked_transaction()?;
        self.add_stock_move(key, qty, &expense.spent_at, "receipt", ("expense", expense_id), &expense.description)?;
        if update_price {
            let amount = expense.amount_cents.abs();
            match key {
                StockKey::Filament { material_id } => {
                    // Preis je kg aus Gesamtbetrag und Gramm
                    let per_kg = (amount * 1000 + qty / 2) / qty;
                    tx.execute("UPDATE material SET price_per_kg_cents = ?1 WHERE id = ?2", params![per_kg, material_id])?;
                }
                StockKey::PurchasePart { purchase_part_id } => {
                    let unit_cost = amount as f64 / 100.0 / qty as f64;
                    tx.execute(
                        "UPDATE purchase_part SET unit_cost_eur = ?1 WHERE id = ?2",
                        params![unit_cost, purchase_part_id],
                    )?;
                }
                StockKey::PrintPart { .. } => {}
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Bucht den Verbrauch einer versendeten Lieferung aus (Fertigteile und Zukaufteile gemäß Stückliste).
    /// Wird bei jeder Änderung der Lieferung neu berechnet; ohne Versanddatum entstehen keine Bewegungen.
    pub(crate) fn sync_shipment_stock(&self, shipment_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM stock_move WHERE ref_type = 'shipment' AND ref_id = ?1", [shipment_id])?;
        let shipped_at: String =
            self.conn.query_row("SELECT shipped_at FROM shipment WHERE id = ?1", [shipment_id], |r| r.get(0))?;
        if shipped_at.is_empty() {
            return Ok(());
        }
        // (Produkt, Material, Zukaufteil, gelieferte Menge)
        type ShippedLine = (Option<i64>, Option<i64>, Option<i64>, i64);
        let lines: Vec<ShippedLine> = {
            let mut stmt = self.conn.prepare(
                "SELECT ol.product_id, ol.material_id, ol.purchase_part_id, sl.qty
                 FROM shipment_line sl JOIN order_line ol ON ol.id = sl.order_line_id WHERE sl.shipment_id = ?1",
            )?;
            let rows = stmt.query_map([shipment_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        let mut needs: HashMap<StockKey, i64> = HashMap::new();
        for (product, material, part, qty) in lines {
            if let Some(product_id) = product {
                let mut stmt = self
                    .conn
                    .prepare("SELECT print_part_id, purchase_part_id, qty FROM product_bom WHERE product_id = ?1")?;
                let bom = stmt.query_map([product_id], |r| {
                    Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, i64>(2)?))
                })?;
                for entry in bom {
                    match entry? {
                        (Some(print_part_id), _, per_set) => {
                            // Fertigteile werden je Farbe geführt; ohne Material gibt es keinen Posten
                            if let Some(material_id) = material {
                                *needs.entry(StockKey::PrintPart { print_part_id, material_id }).or_default() +=
                                    per_set * qty;
                            }
                        }
                        (None, Some(purchase_part_id), per_set) => {
                            *needs.entry(StockKey::PurchasePart { purchase_part_id }).or_default() += per_set * qty;
                        }
                        (None, None, _) => {}
                    }
                }
            } else if let Some(purchase_part_id) = part {
                *needs.entry(StockKey::PurchasePart { purchase_part_id }).or_default() += qty;
            }
        }

        let mut needs: Vec<(StockKey, i64)> = needs.into_iter().collect();
        needs.sort();
        for (key, qty) in needs {
            self.add_stock_move(key, -qty, &shipped_at, "shipment", ("shipment", shipment_id), "")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::{BomItem, BomLine, Customer, Material, PrintPart, Product, PurchasePart},
        finance::Expense,
        orders::{Order, OrderLine},
    };

    struct Setup {
        db: Db,
        material: Material,
        housing: PrintPart,
        sled: PrintPart,
        cup: PurchasePart,
        order: Order,
    }

    fn setup() -> Setup {
        let db = Db::open_in_memory().unwrap();
        let mut material = Material {
            kind: "PETG".into(),
            color: "black".into(),
            min_stock_g: 2000,
            lead_time_days: 7,
            ..Default::default()
        };
        db.save_material(&mut material).unwrap();
        let mut housing = PrintPart { name: "Housing".into(), ..Default::default() };
        let mut sled = PrintPart { name: "Sled".into(), ..Default::default() };
        db.save_print_part(&mut housing).unwrap();
        db.save_print_part(&mut sled).unwrap();
        let mut cup = PurchasePart { name: "Saugnapf".into(), min_stock: 20, sellable: true, ..Default::default() };
        db.save_purchase_part(&mut cup).unwrap();
        let mut product = Product {
            name: "Case Set".into(),
            bom: vec![
                BomLine { item: BomItem::PrintPart(housing.id), qty: 1 },
                BomLine { item: BomItem::PrintPart(sled.id), qty: 1 },
                BomLine { item: BomItem::PurchasePart(cup.id), qty: 4 },
            ],
            ..Default::default()
        };
        db.save_product(&mut product).unwrap();
        let mut customer = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut customer).unwrap();
        let mut order = Order {
            customer_id: customer.id,
            received_at: "2026-10-01".into(),
            lines: vec![
                OrderLine { product_id: Some(product.id), material_id: Some(material.id), qty: 5, unit_price_cents: 1300, ..Default::default() },
                OrderLine { description: "Frei".into(), qty: 1, unit_price_cents: 100, ..Default::default() },
            ],
            ..Default::default()
        };
        db.save_order(&mut order).unwrap();
        Setup { db, material, housing, sled, cup, order }
    }

    #[test]
    fn ledger_balances_inventory_and_manual_moves() {
        let s = setup();
        let db = &s.db;
        let key = StockKey::PurchasePart { purchase_part_id: s.cup.id };
        assert_eq!(db.stock_qty(key).unwrap(), 0);
        assert_eq!(db.stock_inventory(key, 100, "2026-10-01", "Zählung").unwrap(), 100);
        assert_eq!(db.stock_inventory(key, 100, "2026-10-02", "").unwrap(), 0, "keine Differenz, keine Buchung");
        db.add_stock_move(key, -12, "2026-10-03", "scrap", ("", 0), "kaputt").unwrap();
        assert_eq!(db.stock_qty(key).unwrap(), 88);
        assert_eq!(db.stock_inventory(key, 90, "2026-10-04", "").unwrap(), 2);
        assert_eq!(db.stock_moves(key, 10).unwrap().len(), 3);

        assert!(db.add_stock_move(key, 0, "2026-10-01", "manual", ("", 0), "").is_err());
        assert!(db.add_stock_move(key, 1, "kaputt", "manual", ("", 0), "").is_err());
        assert!(db.stock_inventory(key, -1, "2026-10-01", "").is_err());

        let id = db.stock_moves(key, 1).unwrap()[0].id;
        db.delete_stock_move(id).unwrap();
        assert_eq!(db.stock_qty(key).unwrap(), 88);
    }

    #[test]
    fn shipping_books_out_finished_parts_and_purchase_parts_and_follows_edits() {
        let s = setup();
        let db = &s.db;
        let housing = StockKey::PrintPart { print_part_id: s.housing.id, material_id: s.material.id };
        let sled = StockKey::PrintPart { print_part_id: s.sled.id, material_id: s.material.id };
        let cup = StockKey::PurchasePart { purchase_part_id: s.cup.id };
        db.stock_inventory(housing, 10, "2026-10-01", "").unwrap();
        db.stock_inventory(sled, 10, "2026-10-01", "").unwrap();
        db.stock_inventory(cup, 100, "2026-10-01", "").unwrap();

        let product_line = s.order.lines[0].id;
        let mut ship = db.new_shipment_draft(s.order.id).unwrap();
        ship.set_qty(product_line, 3);
        db.save_shipment(&mut ship).unwrap();
        assert_eq!(db.stock_qty(housing).unwrap(), 10, "verpackt, noch nicht versendet");

        ship.shipped_at = "2026-10-05".into();
        db.save_shipment(&mut ship).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(sled).unwrap(), db.stock_qty(cup).unwrap()), (7, 7, 88));

        ship.set_qty(product_line, 5);
        db.save_shipment(&mut ship).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(cup).unwrap()), (5, 80), "wird neu berechnet, nicht doppelt");

        let auto = db.stock_moves(housing, 5).unwrap().into_iter().find(|m| m.reason == "shipment").unwrap();
        assert!(db.delete_stock_move(auto.id).is_err(), "automatische Bewegung nicht von Hand löschbar");

        ship.shipped_at.clear();
        db.save_shipment(&mut ship).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(cup).unwrap()), (10, 100));

        ship.shipped_at = "2026-10-06".into();
        db.save_shipment(&mut ship).unwrap();
        db.delete_shipment(ship.id).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(cup).unwrap()), (10, 100));
    }

    #[test]
    fn receipt_from_expense_books_stock_and_updates_prices() {
        let s = setup();
        let db = &s.db;
        let category = db.list_categories().unwrap()[0].id;
        let mut spool = Expense {
            spent_at: "2026-09-11".into(),
            description: "4 Spulen PETG black".into(),
            amount_cents: 8000,
            category_id: category,
            ..Default::default()
        };
        db.save_expense(&mut spool).unwrap();
        let filament = StockKey::Filament { material_id: s.material.id };
        db.book_receipt(spool.id, filament, 4000, true).unwrap();
        assert_eq!(db.stock_qty(filament).unwrap(), 4000);
        let m = db.list_materials().unwrap().remove(0);
        assert_eq!(m.price_per_kg_cents, 2000);

        let mut cups = Expense { description: "Saugnäpfe".into(), amount_cents: 3000, ..spool.clone() };
        cups.id = 0;
        db.save_expense(&mut cups).unwrap();
        let cup = StockKey::PurchasePart { purchase_part_id: s.cup.id };
        db.book_receipt(cups.id, cup, 100, false).unwrap();
        assert_eq!(db.list_purchase_parts().unwrap()[0].unit_cost_eur, 0.0, "Preis nur auf Wunsch");
        db.book_receipt(cups.id, cup, 100, true).unwrap();
        assert!((db.list_purchase_parts().unwrap()[0].unit_cost_eur - 0.30).abs() < 1e-9);

        let linked = db.stock_moves_for_ref("expense", spool.id).unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].moved_at, "2026-09-11");
        assert!(linked[0].is_manual());

        assert!(db.book_receipt(spool.id, filament, 0, false).is_err());
        let finished = StockKey::PrintPart { print_part_id: s.housing.id, material_id: s.material.id };
        assert!(db.book_receipt(spool.id, finished, 1, false).is_err());
    }

    #[test]
    fn levels_flag_low_stock_and_range() {
        let s = setup();
        let db = &s.db;
        let filament = StockKey::Filament { material_id: s.material.id };
        let cup = StockKey::PurchasePart { purchase_part_id: s.cup.id };
        db.stock_inventory(filament, 1500, "2026-10-01", "").unwrap();
        db.stock_inventory(cup, 100, "2026-10-01", "").unwrap();

        let levels = db.stock_levels().unwrap();
        let f = levels.iter().find(|l| l.key == filament).unwrap();
        assert_eq!((f.qty, f.min_qty, f.unit.as_str()), (1500, 2000, "g"));
        assert!(f.below_min() && f.name == "PETG – black");
        assert_eq!(f.days_left(), None, "ohne Verbrauch keine Reichweite");
        assert_eq!(db.stock_alerts().unwrap().len(), 1);

        // Verbrauch heute: 60 Stück in den letzten 90 Tagen → 100 Stück reichen 150 Tage
        let today = db.today().unwrap();
        db.add_stock_move(cup, -60, &today, "shipment", ("shipment", 99), "").unwrap();
        let c = db.stock_levels().unwrap().into_iter().find(|l| l.key == cup).unwrap();
        assert_eq!((c.qty, c.consumed_90d, c.days_left()), (40, 60, Some(60)));
        assert!(!c.below_min() && !c.reorder_now());
    }
}
