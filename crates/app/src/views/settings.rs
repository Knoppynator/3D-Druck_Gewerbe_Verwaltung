use std::path::PathBuf;

use druckplaner_core::{
    backup::{restore_backup, BackupInfo},
    catalog::Customer,
    import::{read_legacy_ods, LegacyData},
    settings::FIELDS,
    Db,
};
use eframe::egui;

use crate::{icons, theme, widgets};

#[derive(Default)]
pub struct SettingsView {
    /// Werte in der Reihenfolge von [`FIELDS`]
    values: Vec<String>,
    data_dir: PathBuf,
    customers: Vec<Customer>,
    import_ods: String,
    import_pdfs: String,
    import_customer: i64,
    /// Gelesene Altdaten, nach „Prüfen“
    preview: Option<LegacyData>,
    /// Ergebnis des letzten Imports (Zusammenfassung und Hinweise)
    result: Vec<String>,
    /// Vorhandene Sicherungen, neueste zuerst
    backups: Vec<BackupInfo>,
    /// gespeichertes Design
    theme: theme::Mode,
    /// neu gewähltes Design für die Hauptschleife
    theme_change: Option<theme::Mode>,
}

enum Act {
    None,
    Save,
    PickOds,
    PickPdfFolder,
    Check,
    Import,
    BackupNow,
    PickBackupDir,
    OpenBackupDir,
    OpenDataDir,
    Restore,
    Theme(theme::Mode),
}

impl SettingsView {
    pub fn new(data_dir: PathBuf) -> Self {
        SettingsView { data_dir, ..Default::default() }
    }

    pub fn reload(&mut self, db: &Db, status: &mut String) {
        let loaded: druckplaner_core::Result<Vec<String>> =
            FIELDS.iter().map(|f| db.setting_or_default(f.key)).collect();
        if let Some(values) = widgets::report(status, loaded, "") {
            self.values = values;
        }
        if let Some(key) = widgets::report(status, db.setting(theme::Mode::SETTING), "") {
            self.theme = theme::Mode::from_key(key.as_deref().unwrap_or(""));
        }
        if let Some(backups) = widgets::report(status, db.list_backups(), "") {
            self.backups = backups;
        }
        if let Some(customers) = widgets::report(status, db.list_customers(), "") {
            self.customers = customers;
            if self.import_customer == 0 {
                self.import_customer = self.customers.first().map_or(0, |c| c.id);
            }
        }
    }

    fn save(&self, db: &Db) -> druckplaner_core::Result<()> {
        for (f, v) in FIELDS.iter().zip(&self.values) {
            db.set_setting(f.key, v.trim())?;
        }
        Ok(())
    }

    /// Rasterzeilen für die Einstellungen mit diesen Schlüsseln (Reihenfolge wie angegeben).
    fn fields(&mut self, ui: &mut egui::Ui, id: &str, keys: &[&str]) {
        egui::Grid::new(id).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            for key in keys {
                if let Some(i) = FIELDS.iter().position(|f| f.key == *key) {
                    if let Some(v) = self.values.get_mut(i) {
                        widgets::text_row(ui, FIELDS[i].label, v);
                    }
                }
            }
        });
    }

    /// Vom Benutzer gewähltes Design, das die Hauptschleife übernehmen soll.
    pub fn take_theme_change(&mut self) -> Option<theme::Mode> {
        self.theme_change.take()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            widgets::page_header(ui, "Einstellungen", "Absender, Rechnungen, Kalkulation, Darstellung und Datensicherung", |ui| {
                if widgets::primary_button(ui, "Speichern").clicked() {
                    act = Act::Save;
                }
            });

            let wide = ui.available_width() > 980.0;
            let columns = if wide { 2 } else { 1 };
            ui.columns(columns, |cols| {
                {
                    let ui = &mut cols[0];
                    widgets::card(ui, "Absender", "steht auf jeder Rechnung", |ui| {
                        self.fields(ui, "set_sender", &["owner_name", "street", "zip_city", "email", "tax_id", "income_tax_no"]);
                    });
                    widgets::card(ui, "Bankverbindung", "", |ui| {
                        self.fields(ui, "set_bank", &["bank_name", "account_holder", "iban", "bic"]);
                    });
                    widgets::card(ui, "Rechnungen", "", |ui| {
                        self.fields(ui, "set_invoice", &["invoice_last_number", "payment_days", "default_shipping", "invoice_footer", "euer_income_line"]);
                    });
                }
                let ui = &mut cols[columns - 1];
                widgets::card(ui, "Kalkulation und Produktion", "", |ui| {
                    self.fields(ui, "set_calc", &["machine_rate", "labor_rate", "plate_window_from", "plate_window_to", "default_lead_days"]);
                });
                widgets::card(ui, "Darstellung", "", |ui| {
                    ui.horizontal(|ui| {
                        widgets::form_label(ui, "Design");
                        for mode in theme::Mode::ALL {
                            if ui.selectable_label(self.theme == mode, mode.label()).clicked() && self.theme != mode {
                                act = Act::Theme(mode);
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        widgets::form_label(ui, "Datenordner");
                        ui.add(egui::Label::new(self.data_dir.display().to_string()).truncate());
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Datenordner öffnen").clicked() {
                            act = Act::OpenDataDir;
                        }
                        widgets::hint(ui, &format!("Druckplaner {}", env!("CARGO_PKG_VERSION")));
                    });
                });
                widgets::card(ui, "Datensicherung", "beim Beenden, wenn sich etwas geändert hat", |ui| {
                    widgets::hint(
                        ui,
                        "ZIP mit Datenbank, Belegen, Rechnungen, .3mf-Dateien und eigenen Vorlagen. Am besten auf einen anderen \
                         Datenträger (USB-Stick, NAS) sichern.",
                    );
                    self.fields(ui, "set_backup", &["backup_dir", "backup_keep", "backup_auto"]);
                    ui.horizontal_wrapped(|ui| {
                        if widgets::soft_button(ui, "Backup jetzt erstellen").clicked() {
                            act = Act::BackupNow;
                        }
                        if ui.button("Ordner wählen …").clicked() {
                            act = Act::PickBackupDir;
                        }
                        if ui.button("Ordner öffnen").clicked() {
                            act = Act::OpenBackupDir;
                        }
                        if ui
                            .button("Sicherung entpacken …")
                            .on_hover_text("Entpackt eine Sicherung in einen neuen, leeren Ordner – die laufenden Daten bleiben unberührt")
                            .clicked()
                        {
                            act = Act::Restore;
                        }
                    });
                    if self.backups.is_empty() {
                        widgets::hint(ui, "Noch keine Sicherung im Backup-Ordner.");
                    }
                    for b in self.backups.iter().take(5) {
                        ui.horizontal(|ui| {
                            icons::show(ui, icons::Icon::Check, 14.0, theme::p().good_text);
                            ui.label(druckplaner_core::date::format_datetime_de(&b.label()));
                            widgets::hint(ui, &format!("{} MB", widgets::dec(b.size_bytes as f64 / 1_048_576.0, 1)));
                        });
                    }
                });
            });

            widgets::card(ui, "Altdaten importieren", "einmalig", |ui| {
                egui::CollapsingHeader::new("Tabelle und Rechnungs-PDFs übernehmen").default_open(false).show(ui, |ui| {
                    ui.label(
                        "Übernimmt Ausgaben und Rechnungen aus Ausgaben_Tracking.ods (Blätter „Ausgaben“ und „Einnahmen“) \
                         und kopiert die vorhandenen Rechnungs-PDFs. Bereits vorhandene Einträge werden übersprungen.",
                    );
                    ui.add_space(4.0);
                    egui::Grid::new("import_form").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                        widgets::form_label(ui, "Tabelle (.ods)");
                        ui.add(egui::TextEdit::singleline(&mut self.import_ods).desired_width(380.0));
                        if ui.button("Wählen …").clicked() {
                            act = Act::PickOds;
                        }
                        ui.end_row();
                        widgets::form_label(ui, "Ordner mit Rechnungs-PDFs");
                        ui.add(egui::TextEdit::singleline(&mut self.import_pdfs).desired_width(380.0));
                        if ui.button("Wählen …").clicked() {
                            act = Act::PickPdfFolder;
                        }
                        ui.end_row();
                        widgets::form_label(ui, "Kunde der Altrechnungen");
                        egui::ComboBox::from_id_salt("import_customer")
                            .width(380.0)
                            .selected_text(
                                self.customers
                                    .iter()
                                    .find(|c| c.id == self.import_customer)
                                    .map_or("– zuerst unter Kunden anlegen –", |c| c.company.as_str()),
                            )
                            .show_ui(ui, |ui| {
                                for c in &self.customers {
                                    ui.selectable_value(&mut self.import_customer, c.id, c.company.as_str());
                                }
                            });
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Prüfen").clicked() {
                            act = Act::Check;
                        }
                        let ready = self.preview.is_some() && self.import_customer != 0;
                        if ui.add_enabled_ui(ready, |ui| widgets::primary_button(ui, "Importieren")).inner.clicked() {
                            act = Act::Import;
                        }
                    });
                    if let Some(p) = &self.preview {
                        ui.label(format!("Gefunden: {}", p.summary()));
                        for w in &p.warnings {
                            ui.colored_label(ui.visuals().warn_fg_color, w.as_str());
                        }
                    }
                    for (i, line) in self.result.iter().enumerate() {
                        if i == 0 {
                            ui.label(theme::strong(line.as_str(), 14.0));
                        } else {
                            ui.colored_label(ui.visuals().warn_fg_color, line.as_str());
                        }
                    }
                });
            });
        });

        match act {
            Act::None => {}
            Act::Save => {
                widgets::report(status, self.save(db), "Einstellungen gespeichert");
            }
            Act::PickOds => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Tabelle", &["ods"]).pick_file() {
                    self.import_ods = p.display().to_string();
                    self.preview = None;
                }
            }
            Act::PickPdfFolder => {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.import_pdfs = p.display().to_string();
                }
            }
            Act::Theme(mode) => {
                if widgets::report(status, db.set_setting(theme::Mode::SETTING, mode.key()), "Design gespeichert").is_some() {
                    self.theme = mode;
                    self.theme_change = Some(mode);
                }
            }
            Act::OpenDataDir => widgets::open_file(&self.data_dir, status),
            Act::OpenBackupDir => {
                if let Some(dir) = widgets::report(status, db.backup_dir(), "") {
                    let _ = std::fs::create_dir_all(&dir);
                    widgets::open_file(&dir, status);
                }
            }
            Act::PickBackupDir => {
                if let Some(dir) = rfd::FileDialog::new().set_title("Ordner für Sicherungen wählen").pick_folder() {
                    if let Some(i) = FIELDS.iter().position(|f| f.key == "backup_dir") {
                        if let Some(v) = self.values.get_mut(i) {
                            *v = dir.display().to_string();
                        }
                    }
                    if widgets::report(status, self.save(db), "Backup-Ordner gespeichert").is_some() {
                        self.reload(db, status);
                    }
                }
            }
            Act::BackupNow => {
                if let Some(path) = widgets::report(status, db.create_backup(), "") {
                    *status = format!("Sicherung erstellt: {}", path.display());
                    self.reload(db, status);
                }
            }
            Act::Restore => {
                let start = db.backup_dir().unwrap_or_default();
                let Some(zip) = rfd::FileDialog::new().set_title("Sicherung (.zip) wählen").set_directory(start).add_filter("Sicherung", &["zip"]).pick_file() else { return };
                let Some(dest) = rfd::FileDialog::new().set_title("Neuen oder leeren Zielordner wählen").pick_folder() else { return };
                if let Some(count) = widgets::report(status, restore_backup(&zip, &dest), "") {
                    *status = format!(
                        "{count} Dateien nach {} entpackt. Zum Verwenden das Programm mit der Umgebungsvariable DRUCKPLANER_DATA={} starten.",
                        dest.display(),
                        dest.display()
                    );
                }
            }
            Act::Check => {
                self.preview = widgets::report(status, read_legacy_ods(&PathBuf::from(self.import_ods.trim())), "Tabelle gelesen – bitte die Zahlen prüfen");
            }
            Act::Import => {
                if let Some(data) = &self.preview {
                    let pdfs = (!self.import_pdfs.trim().is_empty()).then(|| PathBuf::from(self.import_pdfs.trim()));
                    if let Some(report) =
                        widgets::report(status, db.import_legacy(data, self.import_customer, pdfs.as_deref()), "")
                    {
                        *status = report.summary();
                        self.result = std::iter::once(report.summary()).chain(report.warnings.iter().cloned()).collect();
                        self.preview = None;
                    }
                }
            }
        }
    }
}
