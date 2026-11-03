//! Rechnungen: fortlaufende Nummern `JJJJ-1xxx`, eingefrorener Inhalt, Storno, Zahlungen.

use std::path::PathBuf;

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{Error, Result},
    pdf,
};

/// SQL-Bedingung (Alias `i`): Rechnung ist weder Stornobeleg noch storniert.
pub(crate) const ACTIVE_INVOICE: &str =
    "i.cancels_invoice_id IS NULL AND NOT EXISTS (SELECT 1 FROM invoice x WHERE x.cancels_invoice_id = i.id)";

#[derive(Debug, Clone, PartialEq)]
pub struct InvoiceLine {
    pub description: String,
    pub unit: String,
    /// Bei Stornobelegen negativ
    pub qty: i64,
    pub unit_price_cents: i64,
    /// product, part, shipping, legacy oder other
    pub kind: String,
}

impl InvoiceLine {
    pub fn total_cents(&self) -> i64 {
        self.qty * self.unit_price_cents
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Invoice {
    pub id: i64,
    pub number: String,
    pub customer_id: i64,
    pub shipment_id: Option<i64>,
    pub cancels_invoice_id: Option<i64>,
    pub issued_at: String,
    pub due_at: String,
    pub recipient: String,
    pub salutation: String,
    pub footer: String,
    pub total_cents: i64,
    /// relativ zum Datenordner
    pub pdf_path: String,
    pub lines: Vec<InvoiceLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvoiceState {
    Open,
    Paid,
    Cancelled,
    /// Stornobeleg
    Storno,
}

impl InvoiceState {
    pub fn label(self) -> &'static str {
        match self {
            InvoiceState::Open => "Offen",
            InvoiceState::Paid => "Bezahlt",
            InvoiceState::Cancelled => "Storniert",
            InvoiceState::Storno => "Storno",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InvoiceRow {
    pub id: i64,
    pub number: String,
    pub customer: String,
    pub issued_at: String,
    pub due_at: String,
    pub total_cents: i64,
    pub paid_cents: i64,
    pub state: InvoiceState,
    pub order_id: Option<i64>,
}

impl InvoiceRow {
    pub fn open_cents(&self) -> i64 {
        if self.state == InvoiceState::Open {
            self.total_cents - self.paid_cents
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Payment {
    pub id: i64,
    pub invoice_id: i64,
    pub paid_at: String,
    pub amount_cents: i64,
    pub note: String,
}

pub(crate) fn recipient_text(parts: [&str; 5]) -> String {
    let [company, contact, street, zip, city] = parts;
    let zip_city = format!("{} {}", zip.trim(), city.trim());
    [company, contact, street, zip_city.trim()]
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

impl Db {
    /// Nächste freie Nummer des Jahres: höchste vorhandene (oder in den Einstellungen als
    /// „letzte bisherige“ hinterlegte) + 1, mindestens `JJJJ-1001`.
    pub fn next_invoice_number(&self, year: i32) -> Result<String> {
        let prefix = format!("{year}-");
        let suffix = |n: &str| n.replace('_', "-").strip_prefix(&prefix).and_then(|s| s.trim().parse::<i64>().ok());
        let mut max = 1000;
        let mut stmt = self.conn.prepare("SELECT number FROM invoice WHERE number LIKE ?1")?;
        let numbers = stmt.query_map([format!("{prefix}%")], |r| r.get::<_, String>(0))?;
        for n in numbers {
            if let Some(v) = suffix(&n?) {
                max = max.max(v);
            }
        }
        if let Some(floor) = self.setting("invoice_last_number")? {
            if let Some(v) = suffix(&floor) {
                max = max.max(v);
            }
        }
        Ok(format!("{prefix}{}", max + 1))
    }

    pub fn list_invoices(&self) -> Result<Vec<InvoiceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, i.number, c.company, i.issued_at, i.due_at, i.total_cents,
               (SELECT COALESCE(SUM(amount_cents), 0) FROM payment WHERE invoice_id = i.id),
               i.cancels_invoice_id IS NOT NULL,
               EXISTS (SELECT 1 FROM invoice x WHERE x.cancels_invoice_id = i.id),
               (SELECT order_id FROM shipment WHERE id = i.shipment_id)
             FROM invoice i JOIN customer c ON c.id = i.customer_id
             ORDER BY i.issued_at DESC, i.number DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let total_cents: i64 = r.get(5)?;
            let paid_cents: i64 = r.get(6)?;
            let is_storno: bool = r.get(7)?;
            let cancelled: bool = r.get(8)?;
            let state = if is_storno {
                InvoiceState::Storno
            } else if cancelled {
                InvoiceState::Cancelled
            } else if paid_cents >= total_cents {
                InvoiceState::Paid
            } else {
                InvoiceState::Open
            };
            Ok(InvoiceRow {
                id: r.get(0)?,
                number: r.get(1)?,
                customer: r.get(2)?,
                issued_at: r.get(3)?,
                due_at: r.get(4)?,
                total_cents,
                paid_cents,
                state,
                order_id: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn load_invoice(&self, id: i64) -> Result<Invoice> {
        let mut inv = self.conn.query_row(
            "SELECT id, number, customer_id, shipment_id, cancels_invoice_id, issued_at, due_at,
                    recipient, salutation, footer, total_cents, pdf_path
             FROM invoice WHERE id = ?1",
            [id],
            |r| {
                Ok(Invoice {
                    id: r.get(0)?,
                    number: r.get(1)?,
                    customer_id: r.get(2)?,
                    shipment_id: r.get(3)?,
                    cancels_invoice_id: r.get(4)?,
                    issued_at: r.get(5)?,
                    due_at: r.get(6)?,
                    recipient: r.get(7)?,
                    salutation: r.get(8)?,
                    footer: r.get(9)?,
                    total_cents: r.get(10)?,
                    pdf_path: r.get(11)?,
                    lines: Vec::new(),
                })
            },
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT description, unit, qty, unit_price_cents, kind FROM invoice_line WHERE invoice_id = ?1 ORDER BY pos",
        )?;
        let lines = stmt.query_map([id], |r| {
            Ok(InvoiceLine {
                description: r.get(0)?,
                unit: r.get(1)?,
                qty: r.get(2)?,
                unit_price_cents: r.get(3)?,
                kind: r.get(4)?,
            })
        })?;
        inv.lines = lines.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(inv)
    }

    /// Absoluter Pfad der PDF-Datei einer Rechnung.
    pub fn invoice_pdf_file(&self, inv: &Invoice) -> PathBuf {
        // pdf_path ist mit „/“ gespeichert; Teil für Teil anhängen, damit unter Windows saubere „\“ entstehen
        inv.pdf_path.split('/').fold(self.data_dir().to_path_buf(), |dir, part| dir.join(part))
    }

    fn invoice_template(&self) -> String {
        std::fs::read_to_string(self.data_dir().join("vorlagen").join("rechnung.typ"))
            .unwrap_or_else(|_| pdf::DEFAULT_TEMPLATE.to_string())
    }

    fn storno_target_number(&self, inv: &Invoice) -> Result<Option<String>> {
        match inv.cancels_invoice_id {
            None => Ok(None),
            Some(id) => Ok(Some(
                self.conn.query_row("SELECT number FROM invoice WHERE id = ?1", [id], |r| r.get(0))?,
            )),
        }
    }

    /// Setzt das PDF und schreibt es in `rechnungen/JJJJ/`. Gibt den relativen Pfad zurück.
    fn write_invoice_pdf(&self, inv: &Invoice, storno_of: Option<&str>) -> Result<String> {
        let bytes = pdf::render_invoice(inv, &self.sender()?, storno_of, &self.invoice_template())?;
        let prefix = if storno_of.is_some() { "Storno" } else { "Rechnung" };
        let year = inv.issued_at.get(0..4).unwrap_or("0000");
        let rel = format!("rechnungen/{year}/{prefix}_{}.pdf", inv.number.replace('-', "_"));
        let abs = self.data_dir().join(&rel);
        if let Some(dir) = abs.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&abs, bytes)?;
        Ok(rel)
    }

    /// Vergibt die Nummer, speichert die Rechnung samt Positionen und erzeugt das PDF – alles oder nichts.
    fn issue_invoice(&self, inv: &mut Invoice, storno_of: Option<&str>) -> Result<()> {
        let year = date::year_of(&inv.issued_at)
            .ok_or_else(|| Error::Invalid("Rechnungsdatum ungültig".into()))?;
        let tx = self.conn.unchecked_transaction()?;
        inv.number = self.next_invoice_number(year)?;
        inv.total_cents = inv.lines.iter().map(InvoiceLine::total_cents).sum();
        tx.execute(
            "INSERT INTO invoice (number, customer_id, shipment_id, cancels_invoice_id, issued_at, due_at,
             recipient, salutation, footer, total_cents) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                inv.number, inv.customer_id, inv.shipment_id, inv.cancels_invoice_id, inv.issued_at,
                inv.due_at, inv.recipient, inv.salutation, inv.footer, inv.total_cents
            ],
        )?;
        inv.id = tx.last_insert_rowid();
        for (i, l) in inv.lines.iter().enumerate() {
            tx.execute(
                "INSERT INTO invoice_line (invoice_id, pos, description, unit, qty, unit_price_cents, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![inv.id, i as i64 + 1, l.description, l.unit, l.qty, l.unit_price_cents, l.kind],
            )?;
        }
        let rel = self.write_invoice_pdf(inv, storno_of)?;
        tx.execute("UPDATE invoice SET pdf_path = ?1 WHERE id = ?2", params![rel, inv.id])?;
        if let Err(e) = tx.commit() {
            let _ = std::fs::remove_file(self.data_dir().join(&rel));
            return Err(e.into());
        }
        inv.pdf_path = rel;
        Ok(())
    }

    /// Erstellt die Rechnung zu einer versendeten Lieferung (Rechnungsdatum = Versanddatum).
    pub fn create_invoice(&self, shipment_id: i64) -> Result<i64> {
        let (order_id, shipped_at, charge): (i64, String, i64) = self.conn.query_row(
            "SELECT order_id, shipped_at, shipping_charge_cents FROM shipment WHERE id = ?1",
            [shipment_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if shipped_at.is_empty() {
            return Err(Error::Invalid("Die Lieferung ist noch nicht versendet".into()));
        }
        let existing: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM invoice i WHERE i.shipment_id = ?1 AND {ACTIVE_INVOICE}"),
            [shipment_id],
            |r| r.get(0),
        )?;
        if existing > 0 {
            return Err(Error::Invalid("Für diese Lieferung gibt es bereits eine Rechnung".into()));
        }

        let (customer_id, recipient, salutation): (i64, String, String) = self.conn.query_row(
            "SELECT c.id, c.company, c.contact, c.street, c.zip, c.city, c.salutation
             FROM sales_order o JOIN customer c ON c.id = o.customer_id WHERE o.id = ?1",
            [order_id],
            |r| {
                let company: String = r.get(1)?;
                let contact: String = r.get(2)?;
                let street: String = r.get(3)?;
                let zip: String = r.get(4)?;
                let city: String = r.get(5)?;
                Ok((r.get(0)?, recipient_text([&company, &contact, &street, &zip, &city]), r.get(6)?))
            },
        )?;

        let mut lines: Vec<InvoiceLine> = {
            let mut stmt = self.conn.prepare(
                "SELECT ol.description, ol.unit, sl.qty, ol.unit_price_cents,
                        CASE WHEN ol.product_id IS NOT NULL THEN 'product'
                             WHEN ol.purchase_part_id IS NOT NULL THEN 'part' ELSE 'other' END
                 FROM shipment_line sl JOIN order_line ol ON ol.id = sl.order_line_id
                 WHERE sl.shipment_id = ?1 ORDER BY ol.id",
            )?;
            let rows = stmt.query_map([shipment_id], |r| {
                Ok(InvoiceLine {
                    description: r.get(0)?,
                    unit: r.get(1)?,
                    qty: r.get(2)?,
                    unit_price_cents: r.get(3)?,
                    kind: r.get(4)?,
                })
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if charge > 0 {
            lines.push(InvoiceLine {
                description: "Lieferkosten".into(),
                unit: "Stück".into(),
                qty: 1,
                unit_price_cents: charge,
                kind: "shipping".into(),
            });
        }

        let sender = self.sender()?;
        let mut inv = Invoice {
            customer_id,
            shipment_id: Some(shipment_id),
            due_at: self.add_days(&shipped_at, sender.payment_days)?,
            issued_at: shipped_at,
            recipient,
            salutation: if salutation.trim().is_empty() {
                "Sehr geehrte Damen und Herren,".to_string()
            } else {
                salutation.trim().to_string()
            },
            footer: self.setting_or_default("invoice_footer")?,
            lines,
            ..Default::default()
        };
        self.issue_invoice(&mut inv, None)?;
        Ok(inv.id)
    }

    /// Storniert eine Rechnung durch einen Gegenbeleg; die Lieferung kann danach neu abgerechnet werden.
    pub fn cancel_invoice(&self, id: i64) -> Result<i64> {
        let orig = self.load_invoice(id)?;
        if orig.cancels_invoice_id.is_some() {
            return Err(Error::Invalid("Ein Stornobeleg kann nicht storniert werden".into()));
        }
        let cancelled: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM invoice WHERE cancels_invoice_id = ?1)",
            [id],
            |r| r.get(0),
        )?;
        if cancelled {
            return Err(Error::Invalid("Die Rechnung ist bereits storniert".into()));
        }
        if !self.list_payments(id)?.is_empty() {
            return Err(Error::Invalid(
                "Zur Rechnung sind Zahlungen erfasst – bitte zuerst die Zahlungen entfernen".into(),
            ));
        }
        let today = self.today()?;
        let mut storno = Invoice {
            customer_id: orig.customer_id,
            cancels_invoice_id: Some(id),
            issued_at: today.clone(),
            due_at: today,
            recipient: orig.recipient.clone(),
            salutation: orig.salutation.clone(),
            footer: orig.footer.clone(),
            lines: orig
                .lines
                .iter()
                .map(|l| InvoiceLine { qty: -l.qty, ..l.clone() })
                .collect(),
            ..Default::default()
        };
        self.issue_invoice(&mut storno, Some(&orig.number))?;
        Ok(storno.id)
    }

    /// Erzeugt das PDF einer vorhandenen Rechnung neu (z. B. nach Änderung der Vorlage). Gibt den Pfad zurück.
    pub fn regenerate_invoice_pdf(&self, id: i64) -> Result<PathBuf> {
        let mut inv = self.load_invoice(id)?;
        let storno_of = self.storno_target_number(&inv)?;
        let rel = self.write_invoice_pdf(&inv, storno_of.as_deref())?;
        self.conn.execute("UPDATE invoice SET pdf_path = ?1 WHERE id = ?2", params![rel, id])?;
        inv.pdf_path = rel;
        Ok(self.invoice_pdf_file(&inv))
    }

    // ------------------------------------------------------------ Zahlungen

    pub fn list_payments(&self, invoice_id: i64) -> Result<Vec<Payment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, invoice_id, paid_at, amount_cents, note FROM payment
             WHERE invoice_id = ?1 ORDER BY paid_at, id",
        )?;
        let rows = stmt.query_map([invoice_id], |r| {
            Ok(Payment {
                id: r.get(0)?,
                invoice_id: r.get(1)?,
                paid_at: r.get(2)?,
                amount_cents: r.get(3)?,
                note: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_payment(&self, p: &mut Payment) -> Result<()> {
        if !date::is_valid_iso(&p.paid_at) {
            return Err(Error::Invalid("Zahlungsdatum fehlt oder ist ungültig".into()));
        }
        if p.amount_cents == 0 {
            return Err(Error::Invalid("Betrag fehlt".into()));
        }
        let inv = self.load_invoice(p.invoice_id)?;
        let cancelled: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM invoice WHERE cancels_invoice_id = ?1)",
            [p.invoice_id],
            |r| r.get(0),
        )?;
        if inv.cancels_invoice_id.is_some() || cancelled {
            return Err(Error::Invalid("Für Stornobelege und stornierte Rechnungen gibt es keine Zahlungen".into()));
        }
        self.conn.execute(
            "INSERT INTO payment (invoice_id, paid_at, amount_cents, note) VALUES (?1, ?2, ?3, ?4)",
            params![p.invoice_id, p.paid_at, p.amount_cents, p.note],
        )?;
        p.id = self.conn.last_insert_rowid();
        Ok(())
    }

    pub fn delete_payment(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM payment WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::Customer,
        orders::{Order, OrderLine, OrderStatus},
    };

    /// Bestellung mit 10 Stück à 13,00 € und 2 Stück à 5,00 €, noch nichts geliefert.
    fn setup(db: &Db) -> Order {
        db.set_setting("owner_name", "Nils-Eric Rahm").unwrap();
        db.set_setting("invoice_last_number", "2026-1011").unwrap();
        let mut c = Customer {
            company: "BluePulse Technologies".into(),
            contact: "Erika Mustermann".into(),
            salutation: "Sehr geehrte Frau Mustermann,".into(),
            street: "Musterstraße 1".into(),
            zip: "04109".into(),
            city: "Leipzig".into(),
            ..Default::default()
        };
        db.save_customer(&mut c).unwrap();
        let mut o = Order {
            customer_id: c.id,
            received_at: "2026-09-20".into(),
            lines: vec![
                OrderLine { description: "Case Set\nPETG-black".into(), qty: 10, unit_price_cents: 1300, ..Default::default() },
                OrderLine { description: "O-Ring groß".into(), qty: 2, unit_price_cents: 500, ..Default::default() },
            ],
            ..Default::default()
        };
        db.save_order(&mut o).unwrap();
        o
    }

    #[test]
    fn recipient_skips_empty_parts() {
        assert_eq!(
            recipient_text(["Firma", "", "Weg 1", "04109", "Leipzig"]),
            "Firma\nWeg 1\n04109 Leipzig"
        );
    }

    #[test]
    fn invoice_flow_from_shipment_to_payment_and_storno() {
        let db = Db::open_in_memory().unwrap();
        let o = setup(&db);

        let mut ship = db.new_shipment_draft(o.id).unwrap();
        ship.shipping_charge_cents = 769;
        db.save_shipment(&mut ship).unwrap();
        assert!(db.create_invoice(ship.id).is_err(), "noch nicht versendet");

        ship.shipped_at = "2026-09-30".into();
        db.save_shipment(&mut ship).unwrap();
        let sum = &db.list_order_summaries().unwrap()[0];
        assert_eq!(sum.status(), OrderStatus::Shipped);

        // Nummernkreis knüpft an die bisherige Nummer an
        let inv_id = db.create_invoice(ship.id).unwrap();
        let inv = db.load_invoice(inv_id).unwrap();
        assert_eq!(inv.number, "2026-1012");
        assert_eq!(inv.total_cents, 10 * 1300 + 2 * 500 + 769);
        assert_eq!(inv.due_at, "2026-10-14");
        assert_eq!(inv.lines.len(), 3);
        assert!(inv.recipient.starts_with("BluePulse Technologies\nErika Mustermann\nMusterstraße 1\n04109 Leipzig"));
        let pdf = std::fs::read(db.invoice_pdf_file(&inv)).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let shown = db.invoice_pdf_file(&inv).to_string_lossy().into_owned();
        assert!(!shown.contains('/'), "Pfad muss einheitliche Trenner haben: {shown}");

        assert!(db.create_invoice(ship.id).is_err(), "nur eine gültige Rechnung je Lieferung");
        assert!(db.save_shipment(&mut ship).is_err(), "abgerechnete Lieferung ist gesperrt");
        assert_eq!(db.list_shipments(o.id).unwrap()[0].invoice_number.as_deref(), Some("2026-1012"));
        assert_eq!(db.list_order_summaries().unwrap()[0].status(), OrderStatus::Invoiced);

        // Teilzahlung, dann Rest
        let mut p1 = Payment { invoice_id: inv_id, paid_at: "2026-10-02".into(), amount_cents: 10_000, ..Default::default() };
        db.add_payment(&mut p1).unwrap();
        let row = db.list_invoices().unwrap().remove(0);
        assert_eq!((row.state, row.paid_cents, row.open_cents()), (InvoiceState::Open, 10_000, inv.total_cents - 10_000));
        let mut p2 = Payment {
            invoice_id: inv_id,
            paid_at: "2026-10-05".into(),
            amount_cents: inv.total_cents - 10_000,
            ..Default::default()
        };
        db.add_payment(&mut p2).unwrap();
        assert_eq!(db.list_invoices().unwrap()[0].state, InvoiceState::Paid);
        assert_eq!(db.list_order_summaries().unwrap()[0].status(), OrderStatus::Paid);

        // Storno nur ohne Zahlungen
        assert!(db.cancel_invoice(inv_id).is_err());
        db.delete_payment(p1.id).unwrap();
        db.delete_payment(p2.id).unwrap();
        let storno_id = db.cancel_invoice(inv_id).unwrap();
        let storno = db.load_invoice(storno_id).unwrap();
        assert_eq!(storno.number, "2026-1013");
        assert_eq!(storno.total_cents, -inv.total_cents);
        assert!(std::fs::read(db.invoice_pdf_file(&storno)).unwrap().starts_with(b"%PDF"));
        assert!(db.cancel_invoice(inv_id).is_err());
        assert!(db.cancel_invoice(storno_id).is_err());
        assert!(db.add_payment(&mut Payment { invoice_id: inv_id, paid_at: "2026-10-06".into(), amount_cents: 1, ..Default::default() }).is_err());

        let states: Vec<_> = db.list_invoices().unwrap().iter().map(|r| (r.number.clone(), r.state)).collect();
        assert!(states.contains(&("2026-1012".to_string(), InvoiceState::Cancelled)));
        assert!(states.contains(&("2026-1013".to_string(), InvoiceState::Storno)));

        // Lieferung kann neu abgerechnet werden
        assert_eq!(db.list_order_summaries().unwrap()[0].status(), OrderStatus::Shipped);
        let again = db.create_invoice(ship.id).unwrap();
        assert_eq!(db.load_invoice(again).unwrap().number, "2026-1014");
        db.regenerate_invoice_pdf(again).unwrap();
    }

    #[test]
    fn first_number_of_a_year_starts_at_1001() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.next_invoice_number(2027).unwrap(), "2027-1001");
        db.set_setting("invoice_last_number", "2026_1011").unwrap();
        assert_eq!(db.next_invoice_number(2026).unwrap(), "2026-1012");
        assert_eq!(db.next_invoice_number(2027).unwrap(), "2027-1001");
    }
}
