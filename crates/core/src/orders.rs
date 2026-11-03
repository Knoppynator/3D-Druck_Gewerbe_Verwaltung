//! Bestellungen mit Positionen, Preisvorbelegung aus der Preisliste und abgeleitetem Status.

use rusqlite::params;

use crate::{
    catalog::{Material, Product, PurchasePart},
    date,
    db::Db,
    error::{map_constraint, Error, Result},
    invoicing::ACTIVE_INVOICE,
    pricing,
};

/// Manuelle Produktionsstände bis zur ersten Lieferung (`sales_order.stage`).
pub const STAGES: [&str; 3] = ["Eingegangen", "In Produktion", "Nachbearbeitet"];

#[derive(Debug, Clone, PartialEq)]
pub struct OrderLine {
    pub id: i64,
    pub product_id: Option<i64>,
    pub material_id: Option<i64>,
    pub purchase_part_id: Option<i64>,
    /// Bei Produkt-/Zukaufpositionen wird der Text beim Speichern aus den Stammdaten erzeugt.
    pub description: String,
    pub unit: String,
    pub qty: i64,
    pub unit_price_cents: i64,
}

impl Default for OrderLine {
    fn default() -> Self {
        OrderLine {
            id: 0,
            product_id: None,
            material_id: None,
            purchase_part_id: None,
            description: String::new(),
            unit: "Stück".to_string(),
            qty: 1,
            unit_price_cents: 0,
        }
    }
}

impl OrderLine {
    pub fn total_cents(&self) -> i64 {
        self.qty * self.unit_price_cents
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Order {
    pub id: i64,
    pub customer_id: i64,
    pub received_at: String,
    pub due_date: String,
    pub channel: String,
    pub note: String,
    pub stage: i64,
    pub lines: Vec<OrderLine>,
}

impl Order {
    pub fn total_cents(&self) -> i64 {
        self.lines.iter().map(OrderLine::total_cents).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Received,
    InProduction,
    PostProcessed,
    Packed,
    PartlyShipped,
    Shipped,
    Invoiced,
    Paid,
}

impl OrderStatus {
    pub fn label(self) -> &'static str {
        match self {
            OrderStatus::Received => "Eingegangen",
            OrderStatus::InProduction => "In Produktion",
            OrderStatus::PostProcessed => "Nachbearbeitet",
            OrderStatus::Packed => "Verpackt",
            OrderStatus::PartlyShipped => "Teilweise versendet",
            OrderStatus::Shipped => "Versendet",
            OrderStatus::Invoiced => "Rechnung gestellt",
            OrderStatus::Paid => "Bezahlt",
        }
    }
}

/// Kennzahlen einer Bestellung, aus denen sich der Status ableitet.
#[derive(Debug, Clone, Default)]
pub struct OrderSummary {
    pub id: i64,
    pub customer: String,
    pub received_at: String,
    pub due_date: String,
    pub stage: i64,
    pub ordered_qty: i64,
    pub total_cents: i64,
    pub shipped_qty: i64,
    /// Menge in allen Lieferungen (verpackt oder versendet)
    pub packed_qty: i64,
    /// Versendete Lieferungen ohne gültige Rechnung
    pub uninvoiced_shipments: i64,
    /// Gültige Rechnungen mit offenem Betrag
    pub unpaid_invoices: i64,
}

impl OrderSummary {
    pub fn remaining_qty(&self) -> i64 {
        (self.ordered_qty - self.shipped_qty).max(0)
    }

    pub fn status(&self) -> OrderStatus {
        if self.ordered_qty > 0 && self.shipped_qty >= self.ordered_qty {
            if self.uninvoiced_shipments > 0 {
                OrderStatus::Shipped
            } else if self.unpaid_invoices > 0 {
                OrderStatus::Invoiced
            } else {
                OrderStatus::Paid
            }
        } else if self.shipped_qty > 0 {
            OrderStatus::PartlyShipped
        } else if self.packed_qty > 0 {
            OrderStatus::Packed
        } else {
            match self.stage {
                0 => OrderStatus::Received,
                1 => OrderStatus::InProduction,
                _ => OrderStatus::PostProcessed,
            }
        }
    }
}

/// Menge, nach der die Staffeln (Preis, Lieferzeit) für eine Produktposition greifen: Gesamtmenge des Produkts in der
/// Bestellung oder – wenn das Produkt `tier_per_color` hat – nur die Menge in derselben Farbe.
fn tier_qty(order: &Order, product: &Product, line: &OrderLine) -> i64 {
    order
        .lines
        .iter()
        .filter(|o| o.product_id == line.product_id && (!product.tier_per_color || o.material_id == line.material_id))
        .map(|o| o.qty)
        .sum()
}

/// Setzt Stückpreise aus Preisliste (Produkte, Staffel nach Menge je Produkt bzw. je Produkt und Farbe in der
/// Bestellung) und Verkaufspreis (Zukaufteile). Freie Positionen bleiben unverändert.
pub fn apply_prices(order: &mut Order, products: &[Product], materials: &[Material], parts: &[PurchasePart]) {
    let quantities: Vec<i64> = order
        .lines
        .iter()
        .map(|l| {
            l.product_id
                .and_then(|pid| products.iter().find(|p| p.id == pid))
                .map_or(0, |p| tier_qty(order, p, l))
        })
        .collect();
    for (l, qty) in order.lines.iter_mut().zip(quantities) {
        if let Some(pid) = l.product_id {
            let Some(product) = products.iter().find(|p| p.id == pid) else { continue };
            let kind = l
                .material_id
                .and_then(|mid| materials.iter().find(|m| m.id == mid))
                .map_or("", |m| m.kind.as_str());
            if let Some(cents) = pricing::unit_price_cents(product, kind, qty) {
                l.unit_price_cents = cents;
            }
        } else if let Some(ppid) = l.purchase_part_id {
            if let Some(part) = parts.iter().find(|p| p.id == ppid) {
                l.unit_price_cents = part.sale_price_cents;
            }
        }
    }
}

/// Vorschlag für den Wunschtermin: Eingangsdatum plus die längste Standard-Lieferzeit der Positionen. Produkte
/// verwenden ihre Lieferzeit-Staffel (gleiche Mengenregel wie beim Preis), alles andere `default_days`.
/// `None`, wenn das Eingangsdatum ungültig ist oder keine Lieferzeit bekannt ist.
pub fn suggest_due_date(order: &Order, products: &[Product], default_days: i64) -> Option<String> {
    let fallback = (default_days > 0).then_some(default_days);
    let days = order
        .lines
        .iter()
        .filter_map(|l| {
            let product = l.product_id.and_then(|pid| products.iter().find(|p| p.id == pid));
            match product {
                Some(p) => pricing::lead_days(p, tier_qty(order, p, l)).or(fallback),
                None => fallback,
            }
        })
        .max()?;
    date::shift_days(&order.received_at, days)
}

impl Db {
    pub fn recalc_prices(&self, order: &mut Order) -> Result<()> {
        apply_prices(order, &self.list_products()?, &self.list_materials()?, &self.list_purchase_parts()?);
        Ok(())
    }

    pub fn list_order_summaries(&self) -> Result<Vec<OrderSummary>> {
        let sql = format!(
            "SELECT o.id, c.company, o.received_at, o.due_date, o.stage,
               (SELECT COALESCE(SUM(qty), 0) FROM order_line WHERE order_id = o.id),
               (SELECT COALESCE(SUM(qty * unit_price_cents), 0) FROM order_line WHERE order_id = o.id),
               (SELECT COALESCE(SUM(sl.qty), 0) FROM shipment_line sl JOIN shipment s ON s.id = sl.shipment_id
                 WHERE s.order_id = o.id AND s.shipped_at <> ''),
               (SELECT COALESCE(SUM(sl.qty), 0) FROM shipment_line sl JOIN shipment s ON s.id = sl.shipment_id
                 WHERE s.order_id = o.id),
               (SELECT COUNT(*) FROM shipment s WHERE s.order_id = o.id AND s.shipped_at <> ''
                 AND NOT EXISTS (SELECT 1 FROM invoice i WHERE i.shipment_id = s.id AND {active})),
               (SELECT COUNT(*) FROM invoice i JOIN shipment s ON s.id = i.shipment_id
                 WHERE s.order_id = o.id AND {active}
                 AND i.total_cents > (SELECT COALESCE(SUM(amount_cents), 0) FROM payment WHERE invoice_id = i.id))
             FROM sales_order o JOIN customer c ON c.id = o.customer_id
             ORDER BY o.received_at DESC, o.id DESC",
            active = ACTIVE_INVOICE
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(OrderSummary {
                id: r.get(0)?,
                customer: r.get(1)?,
                received_at: r.get(2)?,
                due_date: r.get(3)?,
                stage: r.get(4)?,
                ordered_qty: r.get(5)?,
                total_cents: r.get(6)?,
                shipped_qty: r.get(7)?,
                packed_qty: r.get(8)?,
                uninvoiced_shipments: r.get(9)?,
                unpaid_invoices: r.get(10)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn load_order(&self, id: i64) -> Result<Order> {
        let mut order = self.conn.query_row(
            "SELECT id, customer_id, received_at, due_date, channel, note, stage FROM sales_order WHERE id = ?1",
            [id],
            |r| {
                Ok(Order {
                    id: r.get(0)?,
                    customer_id: r.get(1)?,
                    received_at: r.get(2)?,
                    due_date: r.get(3)?,
                    channel: r.get(4)?,
                    note: r.get(5)?,
                    stage: r.get(6)?,
                    lines: Vec::new(),
                })
            },
        )?;
        order.lines = self.load_order_lines(id)?;
        Ok(order)
    }

    fn load_order_lines(&self, order_id: i64) -> Result<Vec<OrderLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, product_id, material_id, purchase_part_id, description, unit, qty, unit_price_cents
             FROM order_line WHERE order_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([order_id], |r| {
            Ok(OrderLine {
                id: r.get(0)?,
                product_id: r.get(1)?,
                material_id: r.get(2)?,
                purchase_part_id: r.get(3)?,
                description: r.get(4)?,
                unit: r.get(5)?,
                qty: r.get(6)?,
                unit_price_cents: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Text der Position: Produkt + Material (`PETG-black`), Zukaufteilname oder freier Text.
    fn describe_line(&self, l: &OrderLine) -> Result<String> {
        if let Some(pid) = l.product_id {
            let name: String =
                self.conn.query_row("SELECT name FROM product WHERE id = ?1", [pid], |r| r.get(0))?;
            return Ok(match l.material_id {
                Some(mid) => {
                    let (kind, color): (String, String) = self.conn.query_row(
                        "SELECT kind, color FROM material WHERE id = ?1",
                        [mid],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    format!("{name}\n{kind}-{color}")
                }
                None => name,
            });
        }
        if let Some(ppid) = l.purchase_part_id {
            return Ok(self
                .conn
                .query_row("SELECT name FROM purchase_part WHERE id = ?1", [ppid], |r| r.get(0))?);
        }
        let text = l.description.trim().to_string();
        if text.is_empty() {
            return Err(Error::Invalid("Freie Position ohne Bezeichnung".into()));
        }
        Ok(text)
    }

    pub fn save_order(&self, o: &mut Order) -> Result<()> {
        if o.customer_id == 0 {
            return Err(Error::Invalid("Kunde fehlt".into()));
        }
        if !date::is_valid_iso(&o.received_at) {
            return Err(Error::Invalid("Eingangsdatum fehlt oder ist ungültig".into()));
        }
        if !o.due_date.is_empty() && !date::is_valid_iso(&o.due_date) {
            return Err(Error::Invalid("Wunschtermin ist ungültig".into()));
        }
        if o.lines.is_empty() {
            return Err(Error::Invalid("Die Bestellung hat keine Positionen".into()));
        }
        if o.lines.iter().any(|l| l.qty <= 0) {
            return Err(Error::Invalid("Mengen müssen größer als 0 sein".into()));
        }

        let tx = self.conn.unchecked_transaction()?;
        let id = if o.id == 0 {
            tx.execute(
                "INSERT INTO sales_order (customer_id, received_at, due_date, channel, note, stage)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![o.customer_id, o.received_at, o.due_date, o.channel, o.note, o.stage],
            )?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE sales_order SET customer_id = ?1, received_at = ?2, due_date = ?3, channel = ?4,
                 note = ?5, stage = ?6 WHERE id = ?7",
                params![o.customer_id, o.received_at, o.due_date, o.channel, o.note, o.stage, o.id],
            )?;
            o.id
        };

        let existing: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM order_line WHERE order_id = ?1")?;
            let rows = stmt.query_map([id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for l in &o.lines {
            let description = self.describe_line(l)?;
            if l.id == 0 {
                tx.execute(
                    "INSERT INTO order_line (order_id, product_id, material_id, purchase_part_id,
                     description, unit, qty, unit_price_cents) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![id, l.product_id, l.material_id, l.purchase_part_id, description, l.unit, l.qty, l.unit_price_cents],
                )?;
            } else if existing.contains(&l.id) {
                let delivered: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(qty), 0) FROM shipment_line WHERE order_line_id = ?1",
                    [l.id],
                    |r| r.get(0),
                )?;
                if l.qty < delivered {
                    return Err(Error::Invalid(format!(
                        "Menge {} liegt unter der bereits auf Lieferungen verteilten Menge ({delivered})",
                        l.qty
                    )));
                }
                tx.execute(
                    "UPDATE order_line SET product_id = ?1, material_id = ?2, purchase_part_id = ?3,
                     description = ?4, unit = ?5, qty = ?6, unit_price_cents = ?7 WHERE id = ?8",
                    params![l.product_id, l.material_id, l.purchase_part_id, description, l.unit, l.qty, l.unit_price_cents, l.id],
                )
                .map_err(|e| map_constraint(e, "Position wurde bereits geliefert und kann nicht geändert werden"))?;
            } else {
                return Err(Error::Invalid("Position gehört nicht zu dieser Bestellung".into()));
            }
        }
        for old in existing {
            if !o.lines.iter().any(|l| l.id == old) {
                tx.execute("DELETE FROM order_line WHERE id = ?1", [old])
                    .map_err(|e| map_constraint(e, "Position wurde bereits geliefert und kann nicht entfernt werden"))?;
            }
        }
        tx.commit()?;
        o.id = id;
        o.lines = self.load_order_lines(id)?;
        Ok(())
    }

    /// Setzt den manuellen Produktionsstand (0 Eingegangen … 2 Nachbearbeitet); nur solange noch nichts verpackt ist.
    pub fn set_order_stage(&self, id: i64, stage: i64) -> Result<()> {
        if !(0..STAGES.len() as i64).contains(&stage) {
            return Err(Error::Invalid("Ungültiger Produktionsstand".into()));
        }
        let shipments: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM shipment WHERE order_id = ?1", [id], |r| r.get(0))?;
        if shipments > 0 {
            return Err(Error::Invalid("Die Bestellung hat bereits Lieferungen – ihr Stand ergibt sich daraus".into()));
        }
        self.conn.execute("UPDATE sales_order SET stage = ?1 WHERE id = ?2", params![stage, id])?;
        Ok(())
    }

    pub fn delete_order(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM sales_order WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Bestellung hat bereits Lieferungen und kann nicht gelöscht werden"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Customer, PriceTier, Surcharge};

    fn product() -> Product {
        Product {
            id: 1,
            name: "Set".into(),
            tiers: vec![
                PriceTier { min_qty: 1, unit_price_cents: 2000 },
                PriceTier { min_qty: 10, unit_price_cents: 1300 },
            ],
            surcharges: vec![Surcharge { material_kind: "PETG-CF".into(), cents: 200 }],
            ..Default::default()
        }
    }

    fn material(id: i64, kind: &str) -> Material {
        Material { id, kind: kind.into(), color: "black".into(), ..Default::default() }
    }

    #[test]
    fn tier_uses_total_quantity_across_materials() {
        let mut order = Order {
            lines: vec![
                OrderLine { product_id: Some(1), material_id: Some(1), qty: 8, ..Default::default() },
                OrderLine { product_id: Some(1), material_id: Some(2), qty: 4, ..Default::default() },
                OrderLine { purchase_part_id: Some(7), qty: 3, ..Default::default() },
                OrderLine { description: "Frei".into(), unit_price_cents: 555, ..Default::default() },
            ],
            ..Default::default()
        };
        let parts = [PurchasePart { id: 7, sale_price_cents: 125, ..Default::default() }];
        apply_prices(&mut order, &[product()], &[material(1, "PETG"), material(2, "PETG-CF")], &parts);
        assert_eq!(order.lines[0].unit_price_cents, 1300);
        assert_eq!(order.lines[1].unit_price_cents, 1500);
        assert_eq!(order.lines[2].unit_price_cents, 125);
        assert_eq!(order.lines[3].unit_price_cents, 555);
        assert_eq!(order.total_cents(), 8 * 1300 + 4 * 1500 + 3 * 125 + 555);
    }

    #[test]
    fn tier_per_color_counts_each_color_on_its_own() {
        let mut order = Order {
            lines: vec![
                OrderLine { product_id: Some(1), material_id: Some(1), qty: 8, ..Default::default() },
                OrderLine { product_id: Some(1), material_id: Some(2), qty: 4, ..Default::default() },
                OrderLine { product_id: Some(1), material_id: Some(1), qty: 3, ..Default::default() },
            ],
            ..Default::default()
        };
        let mut p = product();
        p.tier_per_color = true;
        let materials = [material(1, "PETG"), material(2, "PETG-CF")];
        apply_prices(&mut order, &[p], &materials, &[]);
        // Farbe 1: 8 + 3 = 11 Stück → Stufe ab 10; Farbe 2: 4 Stück → Stufe ab 1 plus Aufpreis
        assert_eq!(order.lines[0].unit_price_cents, 1300);
        assert_eq!(order.lines[2].unit_price_cents, 1300);
        assert_eq!(order.lines[1].unit_price_cents, 2000 + 200);
    }

    #[test]
    fn due_date_suggestion_uses_lead_tiers_with_default_fallback() {
        use crate::catalog::LeadTier;
        let mut p = product();
        p.lead_tiers = vec![LeadTier { min_qty: 1, days: 7 }, LeadTier { min_qty: 10, days: 14 }];
        let mut order = Order {
            received_at: "2026-10-20".into(),
            lines: vec![OrderLine { product_id: Some(1), material_id: Some(1), qty: 4, ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(suggest_due_date(&order, &[p.clone()], 21).as_deref(), Some("2026-10-27"));
        order.lines[0].qty = 12;
        assert_eq!(suggest_due_date(&order, &[p.clone()], 21).as_deref(), Some("2026-11-03"));
        // zusätzliche freie Position: gilt die Standard-Lieferzeit, die längste gewinnt
        order.lines.push(OrderLine { description: "Frei".into(), ..Default::default() });
        assert_eq!(suggest_due_date(&order, &[p.clone()], 21).as_deref(), Some("2026-11-10"));
        // ohne Staffel und ohne Standard: kein Vorschlag
        assert_eq!(suggest_due_date(&order, &[product()], 0), None);
        order.received_at.clear();
        assert_eq!(suggest_due_date(&order, &[p], 21), None);
    }

    #[test]
    fn status_is_derived_from_progress() {
        let base = OrderSummary { ordered_qty: 10, ..Default::default() };
        assert_eq!(base.status(), OrderStatus::Received);
        assert_eq!(OrderSummary { stage: 1, ..base.clone() }.status(), OrderStatus::InProduction);
        assert_eq!(OrderSummary { stage: 2, ..base.clone() }.status(), OrderStatus::PostProcessed);
        assert_eq!(OrderSummary { packed_qty: 4, ..base.clone() }.status(), OrderStatus::Packed);
        assert_eq!(OrderSummary { packed_qty: 4, shipped_qty: 4, ..base.clone() }.status(), OrderStatus::PartlyShipped);
        let shipped = OrderSummary { packed_qty: 10, shipped_qty: 10, uninvoiced_shipments: 1, ..base.clone() };
        assert_eq!(shipped.status(), OrderStatus::Shipped);
        let invoiced = OrderSummary { uninvoiced_shipments: 0, unpaid_invoices: 1, ..shipped.clone() };
        assert_eq!(invoiced.status(), OrderStatus::Invoiced);
        let paid = OrderSummary { unpaid_invoices: 0, ..invoiced };
        assert_eq!(paid.status(), OrderStatus::Paid);
        assert_eq!(paid.remaining_qty(), 0);
    }

    #[test]
    fn save_order_builds_descriptions_and_keeps_line_ids() {
        let db = Db::open_in_memory().unwrap();
        let mut customer = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut customer).unwrap();
        let mut mat = Material { kind: "PETG".into(), color: "black".into(), ..Default::default() };
        db.save_material(&mut mat).unwrap();
        let mut prod = Product { name: "Case Set".into(), ..Default::default() };
        db.save_product(&mut prod).unwrap();

        let mut order = Order {
            customer_id: customer.id,
            received_at: "2026-10-01".into(),
            lines: vec![
                OrderLine { product_id: Some(prod.id), material_id: Some(mat.id), qty: 5, unit_price_cents: 1500, ..Default::default() },
                OrderLine { description: "Sonderteil".into(), qty: 1, unit_price_cents: 900, ..Default::default() },
            ],
            ..Default::default()
        };
        db.save_order(&mut order).unwrap();
        assert_eq!(order.lines[0].description, "Case Set\nPETG-black");
        let first_id = order.lines[0].id;
        assert_ne!(first_id, 0);

        order.lines.pop();
        order.lines[0].qty = 6;
        db.save_order(&mut order).unwrap();
        let loaded = db.load_order(order.id).unwrap();
        assert_eq!(loaded.lines.len(), 1);
        assert_eq!(loaded.lines[0].id, first_id);
        assert_eq!(loaded.lines[0].qty, 6);

        let sums = db.list_order_summaries().unwrap();
        assert_eq!(sums.len(), 1);
        assert_eq!(sums[0].total_cents, 6 * 1500);
        assert_eq!(sums[0].status(), OrderStatus::Received);

        db.set_order_stage(order.id, 2).unwrap();
        assert_eq!(db.list_order_summaries().unwrap()[0].status(), OrderStatus::PostProcessed);
        assert!(db.set_order_stage(order.id, 3).is_err());
        let mut ship = db.new_shipment_draft(order.id).unwrap();
        db.save_shipment(&mut ship).unwrap();
        assert!(db.set_order_stage(order.id, 1).is_err(), "mit Lieferung nicht mehr manuell");
        db.delete_shipment(ship.id).unwrap();

        let mut invalid = Order { received_at: "2026-10-01".into(), ..Default::default() };
        assert!(db.save_order(&mut invalid).is_err());
        db.delete_order(order.id).unwrap();
        assert!(db.list_order_summaries().unwrap().is_empty());
    }
}
