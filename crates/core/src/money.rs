//! Geldbeträge werden als ganze Cent (`i64`) geführt.

pub fn eur_to_cents(eur: f64) -> i64 {
    (eur * 100.0).round() as i64
}

pub fn cents_to_eur(cents: i64) -> f64 {
    cents as f64 / 100.0
}

/// Liest einen Euro-Betrag wie `7,69`, `7.69` oder `7,69 €` als Cent; `None` bei ungültigem oder negativem Text.
pub fn parse_eur_cents(s: &str) -> Option<i64> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace() && *c != '€').collect();
    let value: f64 = cleaned.replace(',', ".").parse().ok()?;
    (value.is_finite() && value >= 0.0).then(|| eur_to_cents(value))
}

/// Deutsche Schreibweise, z. B. `1.234,56 €`.
pub fn format_eur(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let (euros, ct) = (abs / 100, abs % 100);
    let digits = euros.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!("{sign}{grouped},{ct:02} €")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_german_style() {
        assert_eq!(format_eur(0), "0,00 €");
        assert_eq!(format_eur(-5), "-0,05 €");
        assert_eq!(format_eur(20269), "202,69 €");
        assert_eq!(format_eur(123456), "1.234,56 €");
        assert_eq!(format_eur(100000000), "1.000.000,00 €");
    }

    #[test]
    fn parses_euro_input() {
        assert_eq!(parse_eur_cents("7,69"), Some(769));
        assert_eq!(parse_eur_cents(" 7.69 € "), Some(769));
        assert_eq!(parse_eur_cents("0"), Some(0));
        assert_eq!(parse_eur_cents(""), None);
        assert_eq!(parse_eur_cents("-1"), None);
        assert_eq!(parse_eur_cents("abc"), None);
    }

    #[test]
    fn converts_between_eur_and_cents() {
        assert_eq!(eur_to_cents(13.0), 1300);
        assert_eq!(eur_to_cents(7.69), 769);
        assert_eq!(eur_to_cents(0.29), 29);
        assert_eq!(cents_to_eur(769), 7.69);
    }
}
