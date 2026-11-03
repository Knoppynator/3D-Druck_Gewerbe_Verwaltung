use crate::catalog::{PriceTier, Product};

/// Stückpreis in Cent für `qty` Stück (Gesamtmenge des Produkts in der Bestellung) im Werkstoff
/// `material_kind`: Staffelpreis der höchsten passenden Stufe plus Werkstoff-Aufpreis.
/// Liegt `qty` unter der kleinsten Stufe, gilt die kleinste Stufe. `None`, wenn keine Staffel gepflegt ist.
pub fn unit_price_cents(product: &Product, material_kind: &str, qty: i64) -> Option<i64> {
    let mut tiers: Vec<&PriceTier> = product.tiers.iter().collect();
    tiers.sort_by_key(|t| t.min_qty);
    let base = tiers
        .iter()
        .rev()
        .find(|t| t.min_qty <= qty)
        .or(tiers.first())?
        .unit_price_cents;
    let surcharge: i64 = product
        .surcharges
        .iter()
        .filter(|s| s.material_kind == material_kind)
        .map(|s| s.cents)
        .sum();
    Some(base + surcharge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Surcharge;

    fn product() -> Product {
        Product {
            tiers: [(1, 2000), (5, 1500), (10, 1300), (20, 1000)]
                .into_iter()
                .map(|(min_qty, unit_price_cents)| PriceTier { min_qty, unit_price_cents })
                .collect(),
            surcharges: vec![Surcharge { material_kind: "PETG-CF".into(), cents: 200 }],
            ..Default::default()
        }
    }

    #[test]
    fn picks_highest_matching_tier() {
        let p = product();
        assert_eq!(unit_price_cents(&p, "PETG", 1), Some(2000));
        assert_eq!(unit_price_cents(&p, "PETG", 4), Some(2000));
        assert_eq!(unit_price_cents(&p, "PETG", 5), Some(1500));
        assert_eq!(unit_price_cents(&p, "PETG", 15), Some(1300));
        assert_eq!(unit_price_cents(&p, "PETG", 500), Some(1000));
    }

    #[test]
    fn adds_material_surcharge() {
        let p = product();
        assert_eq!(unit_price_cents(&p, "PETG-CF", 15), Some(1500));
        assert_eq!(unit_price_cents(&p, "PETG-CF", 1), Some(2200));
    }

    #[test]
    fn below_first_tier_uses_first_and_no_tiers_is_none() {
        let mut p = product();
        p.tiers.remove(0);
        assert_eq!(unit_price_cents(&p, "PETG", 1), Some(1500));
        assert_eq!(unit_price_cents(&Product::default(), "PETG", 1), None);
    }
}
