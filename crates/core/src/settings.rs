use rusqlite::OptionalExtension;

use crate::{db::Db, error::Result};

pub struct SettingField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

/// Alle Einstellungen, die der Benutzer pflegt (Absender, Bank, Rechnungstexte).
pub const FIELDS: &[SettingField] = &[
    SettingField { key: "owner_name", label: "Name (Absender)", default: "" },
    SettingField { key: "street", label: "Straße, Hausnr.", default: "" },
    SettingField { key: "zip_city", label: "PLZ, Ort", default: "" },
    SettingField { key: "email", label: "E-Mail", default: "" },
    SettingField { key: "tax_id", label: "Steuer-ID", default: "" },
    SettingField { key: "income_tax_no", label: "Einkommensteuernummer", default: "" },
    SettingField { key: "bank_name", label: "Kreditinstitut", default: "" },
    SettingField { key: "account_holder", label: "Kontoinhaber", default: "" },
    SettingField { key: "iban", label: "IBAN", default: "" },
    SettingField { key: "bic", label: "BIC", default: "" },
    SettingField { key: "payment_days", label: "Zahlungsziel (Tage)", default: "14" },
    SettingField {
        key: "euer_income_line",
        label: "EÜR: Zeile für Betriebseinnahmen (Kleinunternehmer)",
        default: "",
    },
    SettingField {
        key: "backup_dir",
        label: "Backup-Ordner (leer = Unterordner „backups“)",
        default: "",
    },
    SettingField { key: "backup_keep", label: "Backups aufbewahren (Anzahl)", default: "10" },
    SettingField { key: "backup_auto", label: "Backup beim Beenden (ja/nein)", default: "ja" },
    SettingField {
        key: "machine_rate",
        label: "Maschinenstundensatz Drucker (€/h, Kalkulation)",
        default: "0,73",
    },
    SettingField {
        key: "labor_rate",
        label: "Stundensatz Arbeitszeit (€/h, Kalkulation)",
        default: "65",
    },
    SettingField {
        key: "plate_window_from",
        label: "Plattenwechsel möglich ab (HH:MM)",
        default: "07:00",
    },
    SettingField {
        key: "plate_window_to",
        label: "Plattenwechsel möglich bis (HH:MM)",
        default: "23:00",
    },
    SettingField {
        key: "default_shipping",
        label: "Standard-Versandkosten je Lieferung (€)",
        default: "",
    },
    SettingField {
        key: "default_lead_days",
        label: "Standard-Lieferzeit (Tage, wenn das Produkt keine Staffel hat)",
        default: "14",
    },
    SettingField {
        key: "invoice_last_number",
        label: "Letzte bisherige Rechnungsnr. (z. B. 2026-1011)",
        default: "",
    },
    SettingField {
        key: "invoice_footer",
        label: "Hinweis auf Rechnung",
        default: "Als Kleinunternehmer im Sinne von § 19 Abs. 1 UStG wird Umsatzsteuer nicht berechnet.",
    },
];

/// Absenderdaten für Rechnungen (aus den Einstellungen).
#[derive(Debug, Clone, Default)]
pub struct Sender {
    pub name: String,
    pub street: String,
    pub zip_city: String,
    pub email: String,
    pub tax_id: String,
    pub income_tax_no: String,
    pub bank_name: String,
    pub account_holder: String,
    pub iban: String,
    pub bic: String,
    pub payment_days: i64,
}

impl Db {
    pub fn sender(&self) -> Result<Sender> {
        Ok(Sender {
            name: self.setting_or_default("owner_name")?,
            street: self.setting_or_default("street")?,
            zip_city: self.setting_or_default("zip_city")?,
            email: self.setting_or_default("email")?,
            tax_id: self.setting_or_default("tax_id")?,
            income_tax_no: self.setting_or_default("income_tax_no")?,
            bank_name: self.setting_or_default("bank_name")?,
            account_holder: self.setting_or_default("account_holder")?,
            iban: self.setting_or_default("iban")?,
            bic: self.setting_or_default("bic")?,
            payment_days: self.setting_or_default("payment_days")?.trim().parse().unwrap_or(14),
        })
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    /// Gespeicherter Wert oder der in [`FIELDS`] hinterlegte Standard.
    pub fn setting_or_default(&self, key: &str) -> Result<String> {
        if let Some(v) = self.setting(key)? {
            return Ok(v);
        }
        Ok(FIELDS
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.default.to_string())
            .unwrap_or_default())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_then_override() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.setting("payment_days").unwrap(), None);
        assert_eq!(db.setting_or_default("payment_days").unwrap(), "14");
        db.set_setting("payment_days", "30").unwrap();
        db.set_setting("payment_days", "21").unwrap();
        assert_eq!(db.setting_or_default("payment_days").unwrap(), "21");
    }
}
