//! Lieferungen: eine Bestellung kann in mehreren Teillieferungen versendet werden.

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{map_constraint, Error, Result},
    invoicing::ACTIVE_INVOICE,
    money::parse_eur_cents,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShipmentLine {
    pub order_line_id: i64,
    pub qty: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shipment {
    pub id: i64,
    pub order_id: i64,
    pub packed_at: String,
    /// leer = noch nicht versendet
    pub shipped_at: String,
    pub shipping_charge_cents: i64,
    pub tracking: String,
    pub note: String,
    pub lines: Vec<ShipmentLine>,
    /// Nummer der gültigen Rechnung zu dieser Lieferung (nur lesend)
    pub invoice_number: Option<String>,
    pub invoice_id: Option<i64>,
}

impl Shipment {
    pub fn is_shipped(&self) -> bool {
        !self.shipped_at.is_empty()
    }

    pub fn qty_of(&self, order_line_id: i64) -> i64 {
        self.lines.iter().find(|l| l.order_line_id == order_line_id).map_or(0, |l| l.qty)
    }

    /// Setzt die Menge einer Position; 0 entfernt sie aus der Lieferung.
    pub fn set_qty(&mut self, order_line_id: i64, qty: i64) {
        self.lines.retain(|l| l.order_line_id != order_line_id);
        if qty > 0 {
            self.lines.push(ShipmentLine { order_line_id, qty });
            self.lines.sort_by_key(|l| l.order_line_id);
        }
    }
}

impl Db {
    /// Noch nicht auf Lieferungen verteilte Menge je Bestellposition `(Positions-ID, Rest)`;
    /// die Lieferung `exclude_shipment` wird dabei nicht mitgezählt.
    pub fn remaining_by_line(&self, order_id: i64, exclude_shipment: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ol.id, ol.qty - COALESCE((
                 SELECT SUM(sl.qty) FROM shipment_line sl
                 WHERE sl.order_line_id = ol.id AND sl.shipment_id <> ?2), 0)
             FROM order_line ol WHERE ol.order_id = ?1 ORDER BY ol.id",
        )?;
        let rows = stmt.query_map(params![order_id, exclude_shipment], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Versandkosten aus den Einstellungen, die neue Lieferungen vorbelegen (0, wenn nicht gesetzt).
    pub fn default_shipping_cents(&self) -> Result<i64> {
        Ok(parse_eur_cents(&self.setting_or_default("default_shipping")?).unwrap_or(0))
    }

    /// Neue (noch nicht gespeicherte) Lieferung mit allen Restmengen und den Standard-Versandkosten.
    pub fn new_shipment_draft(&self, order_id: i64) -> Result<Shipment> {
        let lines = self
            .remaining_by_line(order_id, 0)?
            .into_iter()
            .filter(|&(_, rest)| rest > 0)
            .map(|(order_line_id, qty)| ShipmentLine { order_line_id, qty })
            .collect();
        Ok(Shipment {
            order_id,
            packed_at: self.today()?,
            shipping_charge_cents: self.default_shipping_cents()?,
            lines,
            ..Default::default()
        })
    }

    pub fn list_shipments(&self, order_id: i64) -> Result<Vec<Shipment>> {
        let sql = format!(
            "SELECT s.id, s.order_id, s.packed_at, s.shipped_at, s.shipping_charge_cents, s.tracking, s.note,
               (SELECT i.number FROM invoice i WHERE i.shipment_id = s.id AND {active} LIMIT 1),
               (SELECT i.id FROM invoice i WHERE i.shipment_id = s.id AND {active} LIMIT 1)
             FROM shipment s WHERE s.order_id = ?1 ORDER BY s.packed_at, s.id",
            active = ACTIVE_INVOICE
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([order_id], |r| {
            Ok(Shipment {
                id: r.get(0)?,
                order_id: r.get(1)?,
                packed_at: r.get(2)?,
                shipped_at: r.get(3)?,
                shipping_charge_cents: r.get(4)?,
                tracking: r.get(5)?,
                note: r.get(6)?,
                lines: Vec::new(),
                invoice_number: r.get(7)?,
                invoice_id: r.get(8)?,
            })
        })?;
        let mut shipments = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut line_stmt = self
            .conn
            .prepare("SELECT order_line_id, qty FROM shipment_line WHERE shipment_id = ?1 ORDER BY order_line_id")?;
        for s in &mut shipments {
            let lines = line_stmt.query_map([s.id], |r| {
                Ok(ShipmentLine { order_line_id: r.get(0)?, qty: r.get(1)? })
            })?;
            s.lines = lines.collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(shipments)
    }

    fn shipment_has_invoice(&self, shipment_id: i64) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM invoice WHERE shipment_id = ?1",
            [shipment_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn save_shipment(&self, s: &mut Shipment) -> Result<()> {
        if !date::is_valid_iso(&s.packed_at) {
            return Err(Error::Invalid("Packdatum fehlt oder ist ungültig".into()));
        }
        if !s.shipped_at.is_empty() && !date::is_valid_iso(&s.shipped_at) {
            return Err(Error::Invalid("Versanddatum ist ungültig".into()));
        }
        s.lines.retain(|l| l.qty > 0);
        if s.lines.is_empty() {
            return Err(Error::Invalid("Die Lieferung enthält keine Positionen".into()));
        }
        if s.id != 0 && self.shipment_has_invoice(s.id)? {
            return Err(Error::Invalid(
                "Die Lieferung ist bereits abgerechnet. Zum Ändern zuerst die Rechnung stornieren.".into(),
            ));
        }
        let remaining = self.remaining_by_line(s.order_id, s.id)?;
        for l in &s.lines {
            match remaining.iter().find(|(id, _)| *id == l.order_line_id) {
                None => return Err(Error::Invalid("Position gehört nicht zu dieser Bestellung".into())),
                Some(&(_, rest)) if l.qty > rest => {
                    return Err(Error::Invalid(format!(
                        "Menge {} übersteigt die noch offene Menge ({rest})",
                        l.qty
                    )))
                }
                Some(_) => {}
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        let id = if s.id == 0 {
            tx.execute(
                "INSERT INTO shipment (order_id, packed_at, shipped_at, shipping_charge_cents, tracking, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![s.order_id, s.packed_at, s.shipped_at, s.shipping_charge_cents, s.tracking, s.note],
            )?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE shipment SET packed_at = ?1, shipped_at = ?2, shipping_charge_cents = ?3,
                 tracking = ?4, note = ?5 WHERE id = ?6",
                params![s.packed_at, s.shipped_at, s.shipping_charge_cents, s.tracking, s.note, s.id],
            )?;
            s.id
        };
        tx.execute("DELETE FROM shipment_line WHERE shipment_id = ?1", [id])?;
        for l in &s.lines {
            tx.execute(
                "INSERT INTO shipment_line (shipment_id, order_line_id, qty) VALUES (?1, ?2, ?3)",
                params![id, l.order_line_id, l.qty],
            )?;
        }
        // Lagerbuchung (Verbrauch bei Versand) im selben Schritt nachziehen
        self.sync_shipment_stock(id)?;
        tx.commit()?;
        s.id = id;
        Ok(())
    }

    pub fn delete_shipment(&self, id: i64) -> Result<()> {
        if self.shipment_has_invoice(id)? {
            return Err(Error::Invalid(
                "Zur Lieferung gibt es Rechnungen und sie kann nicht gelöscht werden".into(),
            ));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM stock_move WHERE ref_type = 'shipment' AND ref_id = ?1", [id])?;
        tx.execute("DELETE FROM shipment WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Lieferung wird noch verwendet"))?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::Customer,
        orders::{Order, OrderLine},
    };

    fn order_with_two_lines(db: &Db) -> Order {
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        let mut o = Order {
            customer_id: c.id,
            received_at: "2026-10-01".into(),
            lines: vec![
                OrderLine { description: "A".into(), qty: 10, unit_price_cents: 1300, ..Default::default() },
                OrderLine { description: "B".into(), qty: 2, unit_price_cents: 500, ..Default::default() },
            ],
            ..Default::default()
        };
        db.save_order(&mut o).unwrap();
        o
    }

    #[test]
    fn partial_shipments_cannot_exceed_order_quantity() {
        let db = Db::open_in_memory().unwrap();
        let o = order_with_two_lines(&db);
        let (a, b) = (o.lines[0].id, o.lines[1].id);

        let mut draft = db.new_shipment_draft(o.id).unwrap();
        assert_eq!(draft.qty_of(a), 10);
        assert_eq!(draft.qty_of(b), 2);

        draft.set_qty(a, 6);
        draft.set_qty(b, 0);
        db.save_shipment(&mut draft).unwrap();
        assert_ne!(draft.id, 0);

        let mut second = db.new_shipment_draft(o.id).unwrap();
        assert_eq!(second.qty_of(a), 4);
        assert_eq!(second.qty_of(b), 2);
        second.set_qty(a, 5);
        assert!(matches!(db.save_shipment(&mut second), Err(Error::Invalid(_))));
        second.set_qty(a, 4);
        db.save_shipment(&mut second).unwrap();

        assert!(db.new_shipment_draft(o.id).unwrap().lines.is_empty());

        // Bearbeiten der ersten Lieferung zählt ihre eigene Menge nicht doppelt
        draft.shipped_at = "2026-10-05".into();
        db.save_shipment(&mut draft).unwrap();
        let listed = db.list_shipments(o.id).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].shipped_at, "2026-10-05");
        assert_eq!(listed[0].lines, vec![ShipmentLine { order_line_id: a, qty: 6 }]);

        let sum = &db.list_order_summaries().unwrap()[0];
        assert_eq!((sum.ordered_qty, sum.packed_qty, sum.shipped_qty), (12, 12, 6));
        assert_eq!(sum.status(), crate::orders::OrderStatus::PartlyShipped);

        // gelieferte Positionen sind gesperrt, leere Lieferungen löschbar
        let mut edited = o.clone();
        edited.lines.remove(0);
        assert!(db.save_order(&mut edited).is_err());
        assert!(db.delete_order(o.id).is_err());
        db.delete_shipment(second.id).unwrap();
        assert_eq!(db.list_shipments(o.id).unwrap().len(), 1);
    }
}
