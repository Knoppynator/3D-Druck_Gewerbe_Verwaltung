//! Entwicklerwerkzeug zum Aufräumen von Testdaten (nicht Teil der App).
//!
//! ```text
//! cargo run -p druckplaner-core --example bereinigen -- anzeigen  <datenbank>
//! cargo run -p druckplaner-core --example bereinigen -- rechnungen-loeschen <datenbank> <nummer>...
//! ```
//!
//! `rechnungen-loeschen` entfernt die genannten Rechnungen samt Positionen und Zahlungen und löscht danach die
//! zugehörigen PDF-Dateien. Stornobelege und stornierte Rechnungen müssen gemeinsam genannt werden. Vorher unbedingt
//! eine Kopie der Datenbank anlegen und die App schließen.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

fn count(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
}

fn show(conn: &Connection) -> rusqlite::Result<()> {
    for table in [
        "customer", "product", "material", "sales_order", "shipment", "invoice", "payment", "expense", "stock_move",
        "print_job", "plate_template", "asset",
    ] {
        println!("{table:15} {}", count(conn, table)?);
    }

    println!("\nKunden:");
    let mut stmt = conn.prepare("SELECT id, company FROM customer ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, company) = row?;
        println!("  #{id} {company}");
    }

    println!("\nBestellungen:");
    let mut stmt = conn.prepare(
        "SELECT o.id, c.company, o.received_at,
                (SELECT COUNT(*) FROM order_line WHERE order_id = o.id),
                (SELECT COALESCE(SUM(qty * unit_price_cents), 0) FROM order_line WHERE order_id = o.id)
         FROM sales_order o JOIN customer c ON c.id = o.customer_id ORDER BY o.id",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
    })? {
        let (id, company, received, lines, total) = row?;
        println!("  #{id} {company}, Eingang {received}, {lines} Position(en), {:.2} €", total as f64 / 100.0);
    }

    println!("\nLieferungen:");
    let mut stmt = conn.prepare(
        "SELECT s.id, s.order_id, s.packed_at, s.shipped_at, s.shipping_charge_cents,
                (SELECT GROUP_CONCAT(number) FROM invoice WHERE shipment_id = s.id)
         FROM shipment s ORDER BY s.id",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?))
    })? {
        let (id, order, packed, shipped, charge, invoices) = row?;
        println!(
            "  #{id} Bestellung #{order}, gepackt {packed}, versendet {}, Versand {:.2} €, Rechnung(en): {}",
            if shipped.is_empty() { "–" } else { &shipped },
            charge as f64 / 100.0,
            invoices.unwrap_or_else(|| "–".into())
        );
    }

    println!("\nRechnungen:");
    let mut stmt = conn.prepare(
        "SELECT i.id, i.number, i.issued_at, i.total_cents, i.cancels_invoice_id, i.shipment_id, i.pdf_path,
                (SELECT COUNT(*) FROM payment WHERE invoice_id = i.id)
         FROM invoice i ORDER BY i.issued_at, i.number",
    )?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })? {
        let (id, number, issued, total, cancels, shipment, pdf, payments) = row?;
        let kind = cancels.map_or(String::new(), |c| format!(" (Storno zu Rechnung #{c})"));
        println!(
            "  #{id} {number} vom {issued}, {:.2} €{kind}, Lieferung {}, {payments} Zahlung(en), {pdf}",
            total as f64 / 100.0,
            shipment.map_or("–".into(), |s| format!("#{s}"))
        );
    }

    println!("\nAusgaben:");
    let mut stmt = conn.prepare("SELECT id, spent_at, description, amount_cents FROM expense ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))? {
        let (id, date, text, cents) = row?;
        println!("  #{id} {date} {text} {:.2} €", cents as f64 / 100.0);
    }
    Ok(())
}

fn delete_invoices(conn: &mut Connection, data_dir: &Path, numbers: &[String]) -> Result<(), String> {
    let e = |err: rusqlite::Error| err.to_string();
    let mut ids = Vec::new();
    for n in numbers {
        let id: i64 = conn
            .query_row("SELECT id FROM invoice WHERE number = ?1", [n], |r| r.get(0))
            .map_err(|_| format!("Rechnung {n} gibt es nicht"))?;
        ids.push(id);
    }
    // Es dürfen keine weiteren Rechnungen an den genannten hängen (Storno-Beziehung)
    for id in &ids {
        let others: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT id, number FROM invoice WHERE cancels_invoice_id = ?1")
                .map_err(e)?;
            let rows = stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).map_err(e)?;
            let mut out = Vec::new();
            for row in rows {
                let (other, number) = row.map_err(e)?;
                if !ids.contains(&other) {
                    out.push(number);
                }
            }
            out
        };
        if !others.is_empty() {
            return Err(format!("Zur Rechnung #{id} gehört der Stornobeleg {} – bitte mit angeben", others.join(", ")));
        }
    }

    let pdfs: Vec<PathBuf> = {
        let mut out = Vec::new();
        for id in &ids {
            let path: String = conn.query_row("SELECT pdf_path FROM invoice WHERE id = ?1", [id], |r| r.get(0)).map_err(e)?;
            if !path.is_empty() {
                out.push(path.split('/').fold(data_dir.to_path_buf(), |d, p| d.join(p)));
            }
        }
        out
    };

    let tx = conn.transaction().map_err(e)?;
    // Stornobelege zuerst (sie verweisen auf die stornierte Rechnung)
    let mut ordered = ids.clone();
    ordered.sort_by_key(|id| {
        let is_storno: bool = tx.query_row("SELECT cancels_invoice_id IS NOT NULL FROM invoice WHERE id = ?1", [id], |r| r.get(0)).unwrap_or(false);
        !is_storno
    });
    for id in &ordered {
        tx.execute("DELETE FROM payment WHERE invoice_id = ?1", params![id]).map_err(e)?;
        tx.execute("DELETE FROM invoice_line WHERE invoice_id = ?1", params![id]).map_err(e)?;
        tx.execute("DELETE FROM invoice WHERE id = ?1", params![id]).map_err(e)?;
    }
    tx.commit().map_err(e)?;

    for pdf in pdfs {
        match std::fs::remove_file(&pdf) {
            Ok(()) => println!("PDF gelöscht: {}", pdf.display()),
            Err(err) => println!("PDF nicht gelöscht ({}): {err}", pdf.display()),
        }
    }
    println!("{} Rechnung(en) gelöscht.", ordered.len());
    Ok(())
}

/// Entfernt alle Geschäftsdaten (Kunden, Produkte, Bestellungen, Rechnungen, Ausgaben, Lager, Produktion …) samt
/// zugehörigen Dateien. Einstellungen und Ausgaben-Kategorien bleiben erhalten, gemerkte .3mf-Zuordnungen nicht.
fn reset_all(conn: &mut Connection, data_dir: &Path) -> Result<(), String> {
    let e = |err: rusqlite::Error| err.to_string();
    // Reihenfolge: zuerst, was auf anderes verweist (Rechnungen vor Lieferungen, Ausgaben vor Lieferungen usw.)
    const TABLES: &[&str] = &[
        "payment",
        "invoice_line",
        "invoice",
        "stock_move",
        "print_job",
        "shipment_line",
        "asset",
        "expense",
        "shipment",
        "order_line",
        "sales_order",
        "finished_min",
        "plate_template_part",
        "plate_template",
        "product_bom",
        "price_tier",
        "material_surcharge",
        "product",
        "purchase_part",
        "print_part",
        "material",
        "customer",
    ];
    let tx = conn.transaction().map_err(e)?;
    // Stornobelege verweisen auf Rechnungen: zuerst lösen
    tx.execute("UPDATE invoice SET cancels_invoice_id = NULL", []).map_err(e)?;
    for table in TABLES {
        let n = tx.execute(&format!("DELETE FROM {table}"), []).map_err(e)?;
        if n > 0 {
            println!("{table:20} {n} gelöscht");
        }
    }
    let mappings = tx.execute("DELETE FROM setting WHERE key LIKE 'map3mf:%'", []).map_err(e)?;
    tx.commit().map_err(e)?;
    println!("gemerkte .3mf-Zuordnungen: {mappings} gelöscht");

    for folder in ["rechnungen", "belege", "3mf", "exporte"] {
        let path = data_dir.join(folder);
        if path.is_dir() {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => println!("Ordner gelöscht: {}", path.display()),
                Err(err) => println!("Ordner nicht gelöscht ({}): {err}", path.display()),
            }
        }
    }
    let settings: i64 = conn.query_row("SELECT COUNT(*) FROM setting", [], |r| r.get(0)).map_err(e)?;
    let categories: i64 = conn.query_row("SELECT COUNT(*) FROM category", [], |r| r.get(0)).map_err(e)?;
    println!("behalten: {settings} Einstellung(en), {categories} Kategorie(n)");
    conn.execute_batch("VACUUM").map_err(e)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("alles-loeschen") if args.len() == 2 => {
            let db = Path::new(&args[1]);
            let dir = db.parent().unwrap_or(Path::new("."));
            open(&args[1]).map_err(|e| e.to_string()).and_then(|mut c| reset_all(&mut c, dir))
        }
        Some("anzeigen") if args.len() == 2 => open(&args[1]).and_then(|c| show(&c)).map_err(|e| e.to_string()),
        Some("rechnungen-loeschen") if args.len() >= 3 => {
            let db = Path::new(&args[1]);
            let dir = db.parent().unwrap_or(Path::new("."));
            open(&args[1]).map_err(|e| e.to_string()).and_then(|mut c| delete_invoices(&mut c, dir, &args[2..]))
        }
        _ => Err("Aufruf: anzeigen <datenbank> | rechnungen-loeschen <datenbank> <nummer>...".into()),
    };
    if let Err(message) = result {
        eprintln!("Fehler: {message}");
        std::process::exit(1);
    }
}
