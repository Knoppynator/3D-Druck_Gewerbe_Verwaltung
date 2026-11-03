//! Stammdaten: Kunden, Material, Druckteile, Zukaufteile, Produkte mit Stückliste und Preisen.

use rusqlite::params;

use crate::{
    db::Db,
    error::{map_constraint, Error, Result},
};

fn collect<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    Ok(rows.collect::<rusqlite::Result<Vec<T>>>()?)
}

fn require(value: &mut String, what: &str) -> Result<()> {
    *value = value.trim().to_string();
    if value.is_empty() {
        return Err(Error::Invalid(format!("{what} fehlt")));
    }
    Ok(())
}

// ---------------------------------------------------------------- Kunden

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Customer {
    pub id: i64,
    pub company: String,
    pub contact: String,
    pub salutation: String,
    pub street: String,
    pub zip: String,
    pub city: String,
    pub note: String,
}

impl Db {
    pub fn list_customers(&self) -> Result<Vec<Customer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, company, contact, salutation, street, zip, city, note
             FROM customer ORDER BY company COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Customer {
                id: r.get(0)?,
                company: r.get(1)?,
                contact: r.get(2)?,
                salutation: r.get(3)?,
                street: r.get(4)?,
                zip: r.get(5)?,
                city: r.get(6)?,
                note: r.get(7)?,
            })
        })?;
        collect(rows)
    }

    pub fn save_customer(&self, c: &mut Customer) -> Result<()> {
        require(&mut c.company, "Firma / Name")?;
        if c.id == 0 {
            self.conn.execute(
                "INSERT INTO customer (company, contact, salutation, street, zip, city, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![c.company, c.contact, c.salutation, c.street, c.zip, c.city, c.note],
            )?;
            c.id = self.conn.last_insert_rowid();
        } else {
            self.conn.execute(
                "UPDATE customer SET company = ?1, contact = ?2, salutation = ?3, street = ?4,
                 zip = ?5, city = ?6, note = ?7 WHERE id = ?8",
                params![c.company, c.contact, c.salutation, c.street, c.zip, c.city, c.note, c.id],
            )?;
        }
        Ok(())
    }

    pub fn delete_customer(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM customer WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Kunde wird noch verwendet"))?;
        Ok(())
    }
}

// -------------------------------------------------------------- Material

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Material {
    pub id: i64,
    /// Werkstoff, z. B. "PETG" oder "PETG-CF"
    pub kind: String,
    pub color: String,
    pub vendor: String,
    pub price_per_kg_cents: i64,
    pub min_stock_g: i64,
    pub lead_time_days: i64,
}

impl Material {
    pub fn label(&self) -> String {
        format!("{} – {}", self.kind, self.color)
    }
}

impl Db {
    pub fn list_materials(&self) -> Result<Vec<Material>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, color, vendor, price_per_kg_cents, min_stock_g, lead_time_days
             FROM material ORDER BY kind COLLATE NOCASE, color COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Material {
                id: r.get(0)?,
                kind: r.get(1)?,
                color: r.get(2)?,
                vendor: r.get(3)?,
                price_per_kg_cents: r.get(4)?,
                min_stock_g: r.get(5)?,
                lead_time_days: r.get(6)?,
            })
        })?;
        collect(rows)
    }

    /// Alle vorkommenden Werkstoffe (für Aufpreise je Material).
    pub fn material_kinds(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT kind FROM material ORDER BY kind COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        collect(rows)
    }

    pub fn save_material(&self, m: &mut Material) -> Result<()> {
        require(&mut m.kind, "Werkstoff")?;
        require(&mut m.color, "Farbe")?;
        let dup = "Diese Kombination aus Werkstoff und Farbe existiert bereits";
        if m.id == 0 {
            self.conn
                .execute(
                    "INSERT INTO material
                     (kind, color, vendor, price_per_kg_cents, min_stock_g, lead_time_days)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![m.kind, m.color, m.vendor, m.price_per_kg_cents, m.min_stock_g, m.lead_time_days],
                )
                .map_err(|e| map_constraint(e, dup))?;
            m.id = self.conn.last_insert_rowid();
        } else {
            self.conn
                .execute(
                    "UPDATE material SET kind = ?1, color = ?2, vendor = ?3, price_per_kg_cents = ?4,
                     min_stock_g = ?5, lead_time_days = ?6 WHERE id = ?7",
                    params![
                        m.kind, m.color, m.vendor, m.price_per_kg_cents, m.min_stock_g,
                        m.lead_time_days, m.id
                    ],
                )
                .map_err(|e| map_constraint(e, dup))?;
        }
        Ok(())
    }

    pub fn delete_material(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM material WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Material wird noch verwendet"))?;
        Ok(())
    }
}

// ------------------------------------------------------------ Druckteile

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintPart {
    pub id: i64,
    pub name: String,
    pub notes: String,
}

impl Db {
    pub fn list_print_parts(&self) -> Result<Vec<PrintPart>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, notes FROM print_part ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(PrintPart { id: r.get(0)?, name: r.get(1)?, notes: r.get(2)? })
        })?;
        collect(rows)
    }

    pub fn save_print_part(&self, p: &mut PrintPart) -> Result<()> {
        require(&mut p.name, "Name")?;
        let dup = "Ein Druckteil mit diesem Namen existiert bereits";
        if p.id == 0 {
            self.conn
                .execute(
                    "INSERT INTO print_part (name, notes) VALUES (?1, ?2)",
                    params![p.name, p.notes],
                )
                .map_err(|e| map_constraint(e, dup))?;
            p.id = self.conn.last_insert_rowid();
        } else {
            self.conn
                .execute(
                    "UPDATE print_part SET name = ?1, notes = ?2 WHERE id = ?3",
                    params![p.name, p.notes, p.id],
                )
                .map_err(|e| map_constraint(e, dup))?;
        }
        Ok(())
    }

    pub fn delete_print_part(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM print_part WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Druckteil wird noch in einer Stückliste verwendet"))?;
        Ok(())
    }
}

// ----------------------------------------------------------- Zukaufteile

#[derive(Debug, Clone, PartialEq)]
pub struct PurchasePart {
    pub id: i64,
    pub name: String,
    pub unit: String,
    /// Kalkulationswert in Euro (kann Bruchteile von Cent enthalten)
    pub unit_cost_eur: f64,
    /// Verkaufspreis je Einheit in Cent, wenn das Teil einzeln verkauft wird
    pub sale_price_cents: i64,
    pub min_stock: i64,
    pub sellable: bool,
}

impl Default for PurchasePart {
    fn default() -> Self {
        PurchasePart {
            id: 0,
            name: String::new(),
            unit: "Stück".to_string(),
            unit_cost_eur: 0.0,
            sale_price_cents: 0,
            min_stock: 0,
            sellable: false,
        }
    }
}

impl Db {
    pub fn list_purchase_parts(&self) -> Result<Vec<PurchasePart>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, unit, unit_cost_eur, sale_price_cents, min_stock, sellable
             FROM purchase_part ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PurchasePart {
                id: r.get(0)?,
                name: r.get(1)?,
                unit: r.get(2)?,
                unit_cost_eur: r.get(3)?,
                sale_price_cents: r.get(4)?,
                min_stock: r.get(5)?,
                sellable: r.get(6)?,
            })
        })?;
        collect(rows)
    }

    pub fn save_purchase_part(&self, p: &mut PurchasePart) -> Result<()> {
        require(&mut p.name, "Name")?;
        require(&mut p.unit, "Einheit")?;
        let dup = "Ein Zukaufteil mit diesem Namen existiert bereits";
        if p.id == 0 {
            self.conn
                .execute(
                    "INSERT INTO purchase_part (name, unit, unit_cost_eur, sale_price_cents, min_stock, sellable)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![p.name, p.unit, p.unit_cost_eur, p.sale_price_cents, p.min_stock, p.sellable],
                )
                .map_err(|e| map_constraint(e, dup))?;
            p.id = self.conn.last_insert_rowid();
        } else {
            self.conn
                .execute(
                    "UPDATE purchase_part SET name = ?1, unit = ?2, unit_cost_eur = ?3,
                     sale_price_cents = ?4, min_stock = ?5, sellable = ?6 WHERE id = ?7",
                    params![p.name, p.unit, p.unit_cost_eur, p.sale_price_cents, p.min_stock, p.sellable, p.id],
                )
                .map_err(|e| map_constraint(e, dup))?;
        }
        Ok(())
    }

    pub fn delete_purchase_part(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM purchase_part WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Zukaufteil wird noch in einer Stückliste verwendet"))?;
        Ok(())
    }
}

// -------------------------------------------------------------- Produkte

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BomItem {
    PrintPart(i64),
    PurchasePart(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BomLine {
    pub item: BomItem,
    pub qty: i64,
}

/// Staffelpreis: gilt ab `min_qty` Stück.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceTier {
    pub min_qty: i64,
    pub unit_price_cents: i64,
}

/// Standard-Lieferzeit in Tagen: gilt ab `min_qty` Stück.
#[derive(Debug, Clone, PartialEq)]
pub struct LeadTier {
    pub min_qty: i64,
    pub days: i64,
}

/// Aufpreis je Stück für einen Werkstoff.
#[derive(Debug, Clone, PartialEq)]
pub struct Surcharge {
    pub material_kind: String,
    pub cents: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub labor_minutes: i64,
    /// Staffeln (Preis, Lieferzeit) zählen die Menge je Farbe statt der Gesamtmenge des Produkts
    pub tier_per_color: bool,
    pub bom: Vec<BomLine>,
    pub tiers: Vec<PriceTier>,
    pub lead_tiers: Vec<LeadTier>,
    pub surcharges: Vec<Surcharge>,
}

impl Db {
    pub fn list_products(&self) -> Result<Vec<Product>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, labor_minutes, tier_per_color FROM product ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| {
            Ok(Product {
                id: r.get(0)?,
                name: r.get(1)?,
                labor_minutes: r.get(2)?,
                tier_per_color: r.get(3)?,
                ..Default::default()
            })
        })?;
        let mut products = collect(rows)?;
        for p in &mut products {
            p.bom = self.load_bom(p.id)?;
            p.tiers = self.load_tiers(p.id)?;
            p.lead_tiers = self.load_lead_tiers(p.id)?;
            p.surcharges = self.load_surcharges(p.id)?;
        }
        Ok(products)
    }

    fn load_bom(&self, product_id: i64) -> Result<Vec<BomLine>> {
        let mut stmt = self.conn.prepare(
            "SELECT print_part_id, purchase_part_id, qty FROM product_bom
             WHERE product_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([product_id], |r| {
            let print: Option<i64> = r.get(0)?;
            let purchase: Option<i64> = r.get(1)?;
            let item = match (print, purchase) {
                (Some(id), _) => BomItem::PrintPart(id),
                (None, Some(id)) => BomItem::PurchasePart(id),
                (None, None) => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok(BomLine { item, qty: r.get(2)? })
        })?;
        collect(rows)
    }

    fn load_tiers(&self, product_id: i64) -> Result<Vec<PriceTier>> {
        let mut stmt = self.conn.prepare(
            "SELECT min_qty, unit_price_cents FROM price_tier WHERE product_id = ?1 ORDER BY min_qty",
        )?;
        let rows = stmt.query_map([product_id], |r| {
            Ok(PriceTier { min_qty: r.get(0)?, unit_price_cents: r.get(1)? })
        })?;
        collect(rows)
    }

    fn load_lead_tiers(&self, product_id: i64) -> Result<Vec<LeadTier>> {
        let mut stmt = self
            .conn
            .prepare("SELECT min_qty, days FROM lead_tier WHERE product_id = ?1 ORDER BY min_qty")?;
        let rows = stmt.query_map([product_id], |r| Ok(LeadTier { min_qty: r.get(0)?, days: r.get(1)? }))?;
        collect(rows)
    }

    fn load_surcharges(&self, product_id: i64) -> Result<Vec<Surcharge>> {
        let mut stmt = self.conn.prepare(
            "SELECT material_kind, cents FROM material_surcharge
             WHERE product_id = ?1 ORDER BY material_kind COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([product_id], |r| {
            Ok(Surcharge { material_kind: r.get(0)?, cents: r.get(1)? })
        })?;
        collect(rows)
    }

    /// Speichert Produkt samt Stückliste, Staffeln und Aufpreisen in einer Transaktion.
    pub fn save_product(&self, p: &mut Product) -> Result<()> {
        require(&mut p.name, "Produktname")?;
        let tx = self.conn.unchecked_transaction()?;
        let dup = "Ein Produkt mit diesem Namen existiert bereits";
        let id = if p.id == 0 {
            tx.execute(
                "INSERT INTO product (name, labor_minutes, tier_per_color) VALUES (?1, ?2, ?3)",
                params![p.name, p.labor_minutes, p.tier_per_color],
            )
            .map_err(|e| map_constraint(e, dup))?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE product SET name = ?1, labor_minutes = ?2, tier_per_color = ?3 WHERE id = ?4",
                params![p.name, p.labor_minutes, p.tier_per_color, p.id],
            )
            .map_err(|e| map_constraint(e, dup))?;
            p.id
        };

        tx.execute("DELETE FROM product_bom WHERE product_id = ?1", [id])?;
        for line in &p.bom {
            let (print, purchase) = match line.item {
                BomItem::PrintPart(i) => (Some(i), None),
                BomItem::PurchasePart(i) => (None, Some(i)),
            };
            tx.execute(
                "INSERT INTO product_bom (product_id, print_part_id, purchase_part_id, qty)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, print, purchase, line.qty],
            )
            .map_err(|e| map_constraint(e, "Ungültige Stücklistenposition"))?;
        }

        tx.execute("DELETE FROM price_tier WHERE product_id = ?1", [id])?;
        for t in &p.tiers {
            tx.execute(
                "INSERT INTO price_tier (product_id, min_qty, unit_price_cents) VALUES (?1, ?2, ?3)",
                params![id, t.min_qty, t.unit_price_cents],
            )
            .map_err(|e| map_constraint(e, "Staffelpreis ungültig oder Menge doppelt vergeben"))?;
        }

        tx.execute("DELETE FROM lead_tier WHERE product_id = ?1", [id])?;
        for t in &p.lead_tiers {
            tx.execute(
                "INSERT INTO lead_tier (product_id, min_qty, days) VALUES (?1, ?2, ?3)",
                params![id, t.min_qty, t.days],
            )
            .map_err(|e| map_constraint(e, "Lieferzeit-Staffel ungültig oder Menge doppelt vergeben"))?;
        }

        tx.execute("DELETE FROM material_surcharge WHERE product_id = ?1", [id])?;
        for s in &p.surcharges {
            tx.execute(
                "INSERT INTO material_surcharge (product_id, material_kind, cents) VALUES (?1, ?2, ?3)",
                params![id, s.material_kind, s.cents],
            )
            .map_err(|e| map_constraint(e, "Aufpreis doppelt für denselben Werkstoff"))?;
        }

        tx.commit()?;
        p.id = id;
        Ok(())
    }

    pub fn delete_product(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM product WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Produkt wird noch verwendet"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_roundtrip_and_validation() {
        let db = Db::open_in_memory().unwrap();
        let mut c = Customer { company: "  BluePulse Technologies ".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        assert_ne!(c.id, 0);
        assert_eq!(c.company, "BluePulse Technologies");

        c.city = "Leipzig".into();
        db.save_customer(&mut c).unwrap();
        let list = db.list_customers().unwrap();
        assert_eq!(list, vec![c.clone()]);

        let mut empty = Customer::default();
        assert!(matches!(db.save_customer(&mut empty), Err(Error::Invalid(_))));

        db.delete_customer(c.id).unwrap();
        assert!(db.list_customers().unwrap().is_empty());
    }

    #[test]
    fn material_unique_per_kind_and_color() {
        let db = Db::open_in_memory().unwrap();
        let mk = || Material { kind: "PETG".into(), color: "black".into(), ..Default::default() };
        db.save_material(&mut mk()).unwrap();
        assert!(matches!(db.save_material(&mut mk()), Err(Error::Invalid(_))));
        db.save_material(&mut Material { kind: "PETG-CF".into(), color: "black".into(), ..Default::default() })
            .unwrap();
        assert_eq!(db.material_kinds().unwrap(), vec!["PETG", "PETG-CF"]);
    }

    #[test]
    fn product_roundtrip_with_children() {
        let db = Db::open_in_memory().unwrap();
        let mut housing = PrintPart { name: "Housing".into(), ..Default::default() };
        db.save_print_part(&mut housing).unwrap();
        let mut cup = PurchasePart { name: "Saugnapf M4".into(), unit_cost_eur: 0.83, ..Default::default() };
        db.save_purchase_part(&mut cup).unwrap();

        let mut p = Product {
            name: "PaddlePulse Case Set".into(),
            labor_minutes: 18,
            bom: vec![
                BomLine { item: BomItem::PrintPart(housing.id), qty: 1 },
                BomLine { item: BomItem::PurchasePart(cup.id), qty: 4 },
            ],
            tiers: vec![
                PriceTier { min_qty: 10, unit_price_cents: 1300 },
                PriceTier { min_qty: 1, unit_price_cents: 2000 },
            ],
            surcharges: vec![Surcharge { material_kind: "PETG-CF".into(), cents: 200 }],
            lead_tiers: vec![LeadTier { min_qty: 10, days: 14 }, LeadTier { min_qty: 1, days: 7 }],
            tier_per_color: true,
            ..Default::default()
        };
        db.save_product(&mut p).unwrap();

        let loaded = db.list_products().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].bom, p.bom);
        assert_eq!(loaded[0].surcharges, p.surcharges);
        assert!(loaded[0].tier_per_color);
        assert_eq!(loaded[0].lead_tiers.iter().map(|t| (t.min_qty, t.days)).collect::<Vec<_>>(), vec![(1, 7), (10, 14)]);
        // Staffeln kommen nach Menge sortiert zurück
        assert_eq!(loaded[0].tiers[0].min_qty, 1);
        assert_eq!(loaded[0].tiers[1].min_qty, 10);

        // Kinder werden beim erneuten Speichern ersetzt, nicht verdoppelt
        p.bom.pop();
        db.save_product(&mut p).unwrap();
        assert_eq!(db.list_products().unwrap()[0].bom.len(), 1);
    }

    #[test]
    fn used_parts_cannot_be_deleted_and_duplicate_tiers_roll_back() {
        let db = Db::open_in_memory().unwrap();
        let mut housing = PrintPart { name: "Housing".into(), ..Default::default() };
        db.save_print_part(&mut housing).unwrap();
        let mut p = Product {
            name: "Set".into(),
            bom: vec![BomLine { item: BomItem::PrintPart(housing.id), qty: 1 }],
            ..Default::default()
        };
        db.save_product(&mut p).unwrap();
        assert!(matches!(db.delete_print_part(housing.id), Err(Error::Invalid(_))));

        let mut bad = Product {
            name: "Kaputt".into(),
            tiers: vec![
                PriceTier { min_qty: 1, unit_price_cents: 100 },
                PriceTier { min_qty: 1, unit_price_cents: 200 },
            ],
            ..Default::default()
        };
        assert!(db.save_product(&mut bad).is_err());
        assert_eq!(bad.id, 0);
        assert_eq!(db.list_products().unwrap().len(), 1);

        db.delete_product(p.id).unwrap();
        db.delete_print_part(housing.id).unwrap();
    }
}
