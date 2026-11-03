//! Kanban-Board: Spalten und das Verschieben von Bestellungen zwischen ihnen.
//!
//! * Eingegangen / In Produktion / Nachbearbeitet: manueller Produktionsstand.
//! * Verpackt / Versendet: Bestellung hat Lieferungen (verpackt, teilweise oder vollständig versendet).
//! * Rechnung gestellt: alles versendet und abgerechnet, Zahlung steht noch aus.

use crate::{
    db::Db,
    error::{Error, Result},
    orders::{OrderStatus, OrderSummary},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardColumn {
    Received,
    InProduction,
    PostProcessed,
    Shipping,
    Invoiced,
}

impl BoardColumn {
    pub const ALL: [BoardColumn; 5] = [
        BoardColumn::Received,
        BoardColumn::InProduction,
        BoardColumn::PostProcessed,
        BoardColumn::Shipping,
        BoardColumn::Invoiced,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            BoardColumn::Received => "Eingegangen",
            BoardColumn::InProduction => "In Produktion",
            BoardColumn::PostProcessed => "Nachbearbeitet",
            BoardColumn::Shipping => "Verpackt / Versendet",
            BoardColumn::Invoiced => "Rechnung gestellt",
        }
    }

    pub fn prev(self) -> Option<BoardColumn> {
        self.index().checked_sub(1).map(|i| Self::ALL[i])
    }

    pub fn next(self) -> Option<BoardColumn> {
        Self::ALL.get(self.index() + 1).copied()
    }

    fn stage(self) -> Option<i64> {
        match self {
            BoardColumn::Received => Some(0),
            BoardColumn::InProduction => Some(1),
            BoardColumn::PostProcessed => Some(2),
            _ => None,
        }
    }
}

impl OrderSummary {
    /// Spalte im Board; bezahlte Bestellungen stehen nicht mehr darauf.
    pub fn board_column(&self) -> Option<BoardColumn> {
        match self.status() {
            OrderStatus::Received => Some(BoardColumn::Received),
            OrderStatus::InProduction => Some(BoardColumn::InProduction),
            OrderStatus::PostProcessed => Some(BoardColumn::PostProcessed),
            OrderStatus::Packed | OrderStatus::PartlyShipped | OrderStatus::Shipped => Some(BoardColumn::Shipping),
            OrderStatus::Invoiced => Some(BoardColumn::Invoiced),
            OrderStatus::Paid => None,
        }
    }
}

impl Db {
    /// Verschiebt eine Bestellung in eine Spalte und führt dabei die nötigen Schritte aus.
    /// Gibt eine Meldung für die Statuszeile zurück.
    ///
    /// * → Produktionsstand: setzt den Stand; kommt die Bestellung aus „Verpackt“, werden die noch
    ///   nicht versendeten Lieferungen verworfen.
    /// * → Verpackt / Versendet: legt eine Lieferung mit allen Restmengen an (heute gepackt).
    /// * → Rechnung gestellt: markiert Lieferungen als heute versendet und erstellt die Rechnungen.
    pub fn move_order(&self, order_id: i64, target: BoardColumn) -> Result<String> {
        let summary = self
            .list_order_summaries()?
            .into_iter()
            .find(|s| s.id == order_id)
            .ok_or_else(|| Error::Invalid("Bestellung nicht gefunden".into()))?;
        let current = summary
            .board_column()
            .ok_or_else(|| Error::Invalid("Die Bestellung ist bereits bezahlt".into()))?;
        if current == target {
            return Ok(String::new());
        }
        if current == BoardColumn::Invoiced {
            return Err(Error::Invalid(
                "Für die Bestellung gibt es Rechnungen – zurücknehmen geht nur über „Stornieren“ bei den Rechnungen".into(),
            ));
        }

        match target {
            BoardColumn::Received | BoardColumn::InProduction | BoardColumn::PostProcessed => {
                let mut discarded = 0;
                if current == BoardColumn::Shipping {
                    let shipments = self.list_shipments(order_id)?;
                    if shipments.iter().any(|s| s.is_shipped() || s.invoice_id.is_some()) {
                        return Err(Error::Invalid(
                            "Es gibt bereits versendete Lieferungen – die Bestellung kann nicht zurückgestellt werden".into(),
                        ));
                    }
                    for s in &shipments {
                        self.delete_shipment(s.id)?;
                        discarded += 1;
                    }
                }
                let stage = target.stage().unwrap_or(0);
                self.set_order_stage(order_id, stage)?;
                Ok(match discarded {
                    0 => format!("Stand: {}", target.label()),
                    n => format!("Stand: {} ({n} Lieferung(en) verworfen)", target.label()),
                })
            }
            BoardColumn::Shipping => {
                let mut draft = self.new_shipment_draft(order_id)?;
                if draft.lines.is_empty() {
                    return Err(Error::Invalid("Alle Mengen sind bereits auf Lieferungen verteilt".into()));
                }
                self.save_shipment(&mut draft)?;
                Ok("Lieferung mit den Restmengen als verpackt angelegt".to_string())
            }
            BoardColumn::Invoiced => {
                let unassigned: i64 = self.remaining_by_line(order_id, 0)?.iter().map(|(_, rest)| rest).sum();
                if unassigned > 0 {
                    return Err(Error::Invalid(format!(
                        "{unassigned} Stück sind noch keiner Lieferung zugeordnet – zuerst nach „Verpackt / Versendet“ verschieben"
                    )));
                }
                let today = self.today()?;
                let mut numbers = Vec::new();
                for mut s in self.list_shipments(order_id)? {
                    if !s.is_shipped() {
                        s.shipped_at = today.clone();
                        self.save_shipment(&mut s)?;
                    }
                    if s.invoice_id.is_none() {
                        let invoice = self.load_invoice(self.create_invoice(s.id)?)?;
                        numbers.push(invoice.number);
                    }
                }
                Ok(format!("Rechnung(en) erstellt: {}", numbers.join(", ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::Customer,
        orders::{Order, OrderLine},
    };

    fn order(db: &Db) -> Order {
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        let mut o = Order {
            customer_id: c.id,
            received_at: "2026-10-01".into(),
            lines: vec![OrderLine { description: "Set".into(), qty: 5, unit_price_cents: 1300, ..Default::default() }],
            ..Default::default()
        };
        db.save_order(&mut o).unwrap();
        o
    }

    fn column(db: &Db, id: i64) -> Option<BoardColumn> {
        db.list_order_summaries().unwrap().iter().find(|s| s.id == id).and_then(OrderSummary::board_column)
    }

    #[test]
    fn columns_navigate() {
        assert_eq!(BoardColumn::Received.prev(), None);
        assert_eq!(BoardColumn::Received.next(), Some(BoardColumn::InProduction));
        assert_eq!(BoardColumn::Invoiced.next(), None);
        assert_eq!(BoardColumn::Shipping.index(), 3);
    }

    #[test]
    fn card_moves_through_the_whole_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("default_shipping", "7,69").unwrap();
        let o = order(&db);
        assert_eq!(column(&db, o.id), Some(BoardColumn::Received));

        db.move_order(o.id, BoardColumn::InProduction).unwrap();
        assert_eq!(column(&db, o.id), Some(BoardColumn::InProduction));
        assert_eq!(db.move_order(o.id, BoardColumn::InProduction).unwrap(), "");

        // Rechnung geht nicht, solange nichts auf Lieferungen verteilt ist
        assert!(db.move_order(o.id, BoardColumn::Invoiced).is_err());

        db.move_order(o.id, BoardColumn::Shipping).unwrap();
        assert_eq!(column(&db, o.id), Some(BoardColumn::Shipping));
        let ships = db.list_shipments(o.id).unwrap();
        assert_eq!(ships.len(), 1);
        assert_eq!(ships[0].shipping_charge_cents, 769, "Standard-Versandkosten");
        assert!(db.move_order(o.id, BoardColumn::Shipping).unwrap().is_empty());

        // Zurück verwirft die noch nicht versendete Lieferung
        db.move_order(o.id, BoardColumn::PostProcessed).unwrap();
        assert!(db.list_shipments(o.id).unwrap().is_empty());
        assert_eq!(column(&db, o.id), Some(BoardColumn::PostProcessed));

        db.move_order(o.id, BoardColumn::Shipping).unwrap();
        db.move_order(o.id, BoardColumn::Invoiced).unwrap();
        assert_eq!(column(&db, o.id), Some(BoardColumn::Invoiced));
        let ships = db.list_shipments(o.id).unwrap();
        assert!(ships[0].is_shipped() && ships[0].invoice_id.is_some());
        let invoice = db.load_invoice(ships[0].invoice_id.unwrap()).unwrap();
        assert_eq!(invoice.total_cents, 5 * 1300 + 769);

        // Danach nur noch über Storno
        assert!(db.move_order(o.id, BoardColumn::Shipping).is_err());
        assert!(db.move_order(o.id, BoardColumn::Received).is_err());
    }

    #[test]
    fn partly_shipped_cannot_skip_to_invoiced_and_shipped_cannot_go_back() {
        let db = Db::open_in_memory().unwrap();
        let o = order(&db);
        let line = o.lines[0].id;

        let mut first = db.new_shipment_draft(o.id).unwrap();
        first.set_qty(line, 2);
        db.save_shipment(&mut first).unwrap();
        assert_eq!(column(&db, o.id), Some(BoardColumn::Shipping));
        assert!(db.move_order(o.id, BoardColumn::Invoiced).is_err(), "3 Stück ohne Lieferung");

        first.shipped_at = "2026-10-05".into();
        db.save_shipment(&mut first).unwrap();
        assert!(db.move_order(o.id, BoardColumn::Received).is_err(), "bereits versendet");

        // Rest verpacken und alles abrechnen
        db.move_order(o.id, BoardColumn::Invoiced).unwrap_err();
        let mut second = db.new_shipment_draft(o.id).unwrap();
        db.save_shipment(&mut second).unwrap();
        db.move_order(o.id, BoardColumn::Invoiced).unwrap();
        assert_eq!(column(&db, o.id), Some(BoardColumn::Invoiced));
        assert_eq!(db.list_invoices().unwrap().len(), 2);
    }
}
