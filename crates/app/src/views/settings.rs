use std::path::PathBuf;

use druckplaner_core::{
    backup::{restore_backup, BackupInfo},
    catalog::Customer,
    import::{read_legacy_ods, LegacyData},
    settings::FIELDS,
    Db,
};
use eframe::egui;

use crate::widgets;

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

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Einstellungen");
            ui.add_space(8.0);
            egui::Grid::new("settings_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                for (f, v) in FIELDS.iter().zip(self.values.iter_mut()) {
                    widgets::text_row(ui, f.label, v);
                }
            });
            ui.add_space(8.0);
            if ui.button("Speichern").clicked() {
                act = Act::Save;
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.label(format!("Datenordner: {}", self.data_dir.display()));
                if ui.small_button("Öffnen").clicked() {
                    act = Act::OpenDataDir;
                }
            });
            ui.weak(format!("Druckplaner {}", env!("CARGO_PKG_VERSION")));

            ui.add_space(16.0);
            ui.strong("Datensicherung");
            ui.label(
                "Beim Beenden entsteht automatisch eine ZIP-Sicherung (Datenbank, Belege, Rechnungen, .3mf-Dateien, eigene Vorlagen), \
                 sofern sich seit der letzten etwas geändert hat. Wähle als Ziel am besten einen anderen Datenträger (USB-Stick, NAS).",
            );
            ui.horizontal(|ui| {
                if ui.button("Backup jetzt erstellen").clicked() {
                    act = Act::BackupNow;
                }
                if ui.button("Backup-Ordner wählen …").clicked() {
                    act = Act::PickBackupDir;
                }
                if ui.button("Backup-Ordner öffnen").clicked() {
                    act = Act::OpenBackupDir;
                }
                if ui.button("Sicherung entpacken …").on_hover_text("Entpackt eine Sicherung in einen neuen, leeren Ordner – die laufenden Daten bleiben unberührt").clicked() {
                    act = Act::Restore;
                }
            });
            if self.backups.is_empty() {
                ui.weak("Noch keine Sicherung im Backup-Ordner.");
            }
            for b in self.backups.iter().take(5) {
                ui.label(format!("{} · {} MB", b.label(), widgets::dec(b.size_bytes as f64 / 1_048_576.0, 1)));
            }

            ui.add_space(16.0);
            egui::CollapsingHeader::new("Altdaten importieren (einmalig)").default_open(false).show(ui, |ui| {
                ui.label(
                    "Übernimmt Ausgaben und Rechnungen aus Ausgaben_Tracking.ods (Blätter „Ausgaben“ und „Einnahmen“) \
                     und kopiert die vorhandenen Rechnungs-PDFs. Bereits vorhandene Einträge werden übersprungen.",
                );
                ui.add_space(4.0);
                egui::Grid::new("import_form").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Tabelle (.ods)");
                    ui.add(egui::TextEdit::singleline(&mut self.import_ods).desired_width(380.0));
                    if ui.button("Wählen …").clicked() {
                        act = Act::PickOds;
                    }
                    ui.end_row();
                    ui.label("Ordner mit Rechnungs-PDFs");
                    ui.add(egui::TextEdit::singleline(&mut self.import_pdfs).desired_width(380.0));
                    if ui.button("Wählen …").clicked() {
                        act = Act::PickPdfFolder;
                    }
                    ui.end_row();
                    ui.label("Kunde der Altrechnungen");
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
                    if ui.add_enabled(self.preview.is_some() && self.import_customer != 0, egui::Button::new("Importieren")).clicked() {
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
                        ui.strong(line.as_str());
                    } else {
                        ui.colored_label(ui.visuals().warn_fg_color, line.as_str());
                    }
                }
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
