use crate::catalog::Product;

/// Wert der höchsten Stufe mit `min_qty <= qty`; liegt `qty` unter der kleinsten Stufe, gilt die kleinste.
fn pick_tier<T>(tiers: &[T], qty: i64, min_qty: impl Fn(&T) -> i64) -> Option<&T> {
    let mut sorted: Vec<&T> = tiers.iter().collect();
    sorted.sort_by_key(|t| min_qty(t));
    sorted.iter().rev().find(|t| min_qty(t) <= qty).or(sorted.first()).copied()
}

/// Standard-Lieferzeit in Tagen für `qty` Stück; `None`, wenn für das Produkt keine Staffel gepflegt ist.
pub fn lead_days(product: &Product, qty: i64) -> Option<i64> {
    pick_tier(&product.lead_tiers, qty, |t| t.min_qty).map(|t| t.days)
}

/// Stückpreis in Cent für `qty` Stück (Menge des Produkts in der Bestellung, je nach Einstellung des Produkts
/// gesamt oder je Farbe) im Werkstoff `material_kind`: Staffelpreis der höchsten passenden Stufe plus
/// Werkstoff-Aufpreis. Liegt `qty` unter der kleinsten Stufe, gilt die kleinste Stufe. `None`, wenn keine
/// Staffel gepflegt ist.
pub fn unit_price_cents(product: &Product, material_kind: &str, qty: i64) -> Option<i64> {
    let base = pick_tier(&product.tiers, qty, |t| t.min_qty)?.unit_price_cents;
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
    use crate::catalog::{PriceTier, Surcharge};

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
    fn lead_days_follow_the_same_tier_rule() {
        use crate::catalog::LeadTier;
        let mut p = product();
        assert_eq!(lead_days(&p, 5), None);
        p.lead_tiers = vec![LeadTier { min_qty: 10, days: 14 }, LeadTier { min_qty: 1, days: 7 }];
        assert_eq!(lead_days(&p, 1), Some(7));
        assert_eq!(lead_days(&p, 9), Some(7));
        assert_eq!(lead_days(&p, 10), Some(14));
        assert_eq!(lead_days(&p, 0), Some(7));
    }

    #[test]
    fn below_first_tier_uses_first_and_no_tiers_is_none() {
        let mut p = product();
        p.tiers.remove(0);
        assert_eq!(unit_price_cents(&p, "PETG", 1), Some(1500));
        assert_eq!(unit_price_cents(&Product::default(), "PETG", 1), None);
    }
}
