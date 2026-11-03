//! „Was ist offen?“: Bestellungen, Versand, fehlende Rechnungen und offene Forderungen.

use crate::{
    db::Db,
    error::Result,
    invoicing::{InvoiceState, ACTIVE_INVOICE},
    orders::OrderStatus,
    stock::StockLevel,
};

#[derive(Debug, Clone)]
pub struct OpenOrder {
    pub order_id: i64,
    pub customer: String,
    pub received_at: String,
    pub due_date: String,
    pub remaining_qty: i64,
    pub status: OrderStatus,
}

#[derive(Debug, Clone)]
pub struct OpenShipment {
    pub shipment_id: i64,
    pub order_id: i64,
    pub customer: String,
    pub packed_at: String,
    pub shipped_at: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone)]
pub struct OpenInvoice {
    pub invoice_id: i64,
    pub order_id: Option<i64>,
    pub number: String,
    pub customer: String,
    pub issued_at: String,
    pub due_at: String,
    pub open_cents: i64,
    /// Tage seit Fälligkeit; negativ = noch nicht fällig
    pub overdue_days: i64,
}

#[derive(Debug, Clone, Default)]
pub struct OpenItems {
    /// Noch nicht vollständig versendete Bestellungen, dringendste zuerst
    pub orders: Vec<OpenOrder>,
    /// Verpackte, aber nicht versendete Lieferungen
    pub to_ship: Vec<OpenShipment>,
    /// Versendete Lieferungen ohne gültige Rechnung
    pub to_invoice: Vec<OpenShipment>,
    /// Gültige Rechnungen mit offenem Betrag
    pub to_collect: Vec<OpenInvoice>,
    /// Lagerposten unter Mindestbestand oder mit knapper Reichweite
    pub low_stock: Vec<StockLevel>,
}

impl Db {
    pub fn open_items(&self) -> Result<OpenItems> {
        let mut orders: Vec<OpenOrder> = self
            .list_order_summaries()?
            .into_iter()
            .filter(|s| s.remaining_qty() > 0)
            .map(|s| OpenOrder {
                order_id: s.id,
                customer: s.customer.clone(),
                received_at: s.received_at.clone(),
                due_date: s.due_date.clone(),
                remaining_qty: s.remaining_qty(),
                status: s.status(),
            })
            .collect();
        // Termine zuerst (früheste oben), ohne Termin nach Eingang
        orders.sort_by(|a, b| {
            (a.due_date.is_empty(), &a.due_date, &a.received_at).cmp(&(b.due_date.is_empty(), &b.due_date, &b.received_at))
        });

        let to_ship = self.open_shipments("s.shipped_at = ''".to_string())?;
        let to_invoice = self.open_shipments(format!(
            "s.shipped_at <> '' AND NOT EXISTS (SELECT 1 FROM invoice i WHERE i.shipment_id = s.id AND {ACTIVE_INVOICE})"
        ))?;

        let today = self.today()?;
        let mut to_collect = Vec::new();
        for row in self.list_invoices()? {
            if row.state == InvoiceState::Open {
                to_collect.push(OpenInvoice {
                    invoice_id: row.id,
                    order_id: row.order_id,
                    open_cents: row.open_cents(),
                    overdue_days: self.days_between(&row.due_at, &today)?,
                    number: row.number,
                    customer: row.customer,
                    issued_at: row.issued_at,
                    due_at: row.due_at,
                });
            }
        }
        to_collect.sort_by_key(|i| std::cmp::Reverse(i.overdue_days));

        Ok(OpenItems { orders, to_ship, to_invoice, to_collect, low_stock: self.stock_alerts()? })
    }

    fn open_shipments(&self, condition: String) -> Result<Vec<OpenShipment>> {
        let sql = format!(
            "SELECT s.id, s.order_id, c.company, s.packed_at, s.shipped_at,
               COALESCE((SELECT SUM(sl.qty * ol.unit_price_cents) FROM shipment_line sl
                         JOIN order_line ol ON ol.id = sl.order_line_id WHERE sl.shipment_id = s.id), 0)
               + s.shipping_charge_cents
             FROM shipment s JOIN sales_order o ON o.id = s.order_id JOIN customer c ON c.id = o.customer_id
             WHERE {condition} ORDER BY s.packed_at, s.id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(OpenShipment {
                shipment_id: r.get(0)?,
                order_id: r.get(1)?,
                customer: r.get(2)?,
                packed_at: r.get(3)?,
                shipped_at: r.get(4)?,
                amount_cents: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::Customer,
        orders::{Order, OrderLine},
    };

    #[test]
    fn open_items_follow_the_order_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        let mut o = Order {
            customer_id: c.id,
            received_at: "2026-09-20".into(),
            lines: vec![OrderLine { description: "Set".into(), qty: 4, unit_price_cents: 1300, ..Default::default() }],
            ..Default::default()
        };
        db.save_order(&mut o).unwrap();

        let items = db.open_items().unwrap();
        assert_eq!((items.orders.len(), items.orders[0].remaining_qty), (1, 4));
        assert!(items.to_ship.is_empty() && items.to_invoice.is_empty() && items.to_collect.is_empty());

        let mut s = db.new_shipment_draft(o.id).unwrap();
        s.shipping_charge_cents = 500;
        db.save_shipment(&mut s).unwrap();
        let items = db.open_items().unwrap();
        assert_eq!(items.to_ship.len(), 1);
        assert_eq!(items.to_ship[0].amount_cents, 4 * 1300 + 500);

        s.shipped_at = "2026-09-30".into();
        db.save_shipment(&mut s).unwrap();
        let items = db.open_items().unwrap();
        assert!(items.orders.is_empty() && items.to_ship.is_empty());
        assert_eq!(items.to_invoice.len(), 1);

        let inv = db.create_invoice(s.id).unwrap();
        let items = db.open_items().unwrap();
        assert!(items.to_invoice.is_empty());
        assert_eq!(items.to_collect.len(), 1);
        assert_eq!(items.to_collect[0].invoice_id, inv);
        assert_eq!(items.to_collect[0].open_cents, 5700);
        assert!(items.to_collect[0].overdue_days > 0, "Fälligkeit 2026-10-14 liegt in der Vergangenheit");
    }
}
