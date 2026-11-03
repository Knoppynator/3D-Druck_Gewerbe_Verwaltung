use druckplaner_core::{
    date::format_de,
    finance::EuerReport,
    money::format_eur,
    Db,
};
use eframe::egui;

use crate::widgets;

enum Act {
    None,
    Year(i32),
    ExportCsv,
    ExportPdf,
}

#[derive(Default)]
pub struct Euer {
    years: Vec<i32>,
    year: i32,
    report: EuerReport,
}

impl Euer {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(years) = widgets::report(status, db.finance_years(), "") {
            self.years = years;
        }
        if self.year == 0 {
            // neuestes Jahr mit Buchungen, sonst das laufende
            let current = widgets::report(status, db.today(), "")
                .and_then(|t| t.get(0..4).and_then(|y| y.parse().ok()))
                .unwrap_or(2026);
            self.year = self.years.first().copied().unwrap_or(current);
        }
        if let Some(r) = widgets::report(status, db.euer_report(self.year), "") {
            self.report = r;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;
        let r = &self.report;
        let error = ui.visuals().error_fg_color;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Einnahmen-Überschuss-Rechnung");
            ui.horizontal(|ui| {
                ui.label("Jahr");
                egui::ComboBox::from_id_salt("euer_year").selected_text(self.year.to_string()).show_ui(ui, |ui| {
                    let mut years = self.years.clone();
                    if !years.contains(&self.year) {
                        years.push(self.year);
                        years.sort_unstable_by(|a, b| b.cmp(a));
                    }
                    for y in years {
                        if ui.selectable_label(self.year == y, y.to_string()).clicked() {
                            act = Act::Year(y);
                        }
                    }
                });
                if ui.button("PDF exportieren").clicked() {
                    act = Act::ExportPdf;
                }
                if ui.button("CSV exportieren").clicked() {
                    act = Act::ExportCsv;
                }
            });
            ui.add_space(8.0);

            egui::Grid::new("euer_summary").striped(true).spacing([16.0, 5.0]).show(ui, |ui| {
                ui.strong("Zeile");
                ui.strong("Position");
                ui.strong("Betrag");
                ui.end_row();
                ui.label(r.income_line.as_str());
                ui.label(r.income_label.as_str());
                ui.label(format_eur(r.income_total_cents));
                ui.end_row();
                for c in &r.by_category {
                    ui.label(c.euer_line.as_str());
                    let label = if c.euer_label.is_empty() { &c.category } else { &c.euer_label };
                    ui.label(format!("{label} ({}, {} Buchungen)", c.category, c.count));
                    ui.label(format_eur(-c.total_cents));
                    ui.end_row();
                }
                ui.label("");
                ui.strong("Summe Betriebsausgaben");
                ui.strong(format_eur(r.expense_total_cents));
                ui.end_row();
                ui.label("");
                ui.strong("Gewinn / Verlust");
                ui.strong(format_eur(r.profit_cents));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.weak("Einnahmen zählen nach Zahlungsdatum. EÜR-Zeilen unter Stammdaten › Kategorien und Einstellungen pflegen.");
            if r.missing_receipts > 0 {
                ui.colored_label(error, format!("Zu {} Ausgaben fehlt der Beleg.", r.missing_receipts));
            }

            ui.add_space(10.0);
            egui::CollapsingHeader::new(format!("Einnahmen ({})", r.income.len())).default_open(false).show(ui, |ui| {
                egui::Grid::new("euer_income").striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                    for i in &r.income {
                        ui.label(format_de(&i.paid_at));
                        ui.label(i.invoice_number.as_str());
                        ui.label(i.customer.as_str());
                        ui.label(format_eur(i.amount_cents));
                        ui.end_row();
                    }
                });
            });
            egui::CollapsingHeader::new(format!("Ausgaben ({})", r.expenses.len())).default_open(false).show(ui, |ui| {
                egui::Grid::new("euer_expenses").striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                    for e in &r.expenses {
                        let x = &e.expense;
                        ui.label(format_de(&x.spent_at));
                        ui.label(x.description.as_str());
                        ui.label(e.category.as_str());
                        ui.label(format_eur(x.amount_cents));
                        if x.has_receipt() {
                            ui.label(x.receipt_label());
                        } else {
                            ui.colored_label(error, "Beleg fehlt");
                        }
                        ui.end_row();
                    }
                });
            });
        });

        match act {
            Act::None => {}
            Act::Year(y) => {
                self.year = y;
                self.reload(db, status);
            }
            Act::ExportCsv => {
                if let Some(path) = widgets::report(status, db.export_euer_csv(self.year), "") {
                    *status = format!("CSV gespeichert: {}", path.display());
                    widgets::open_file(&path, status);
                }
            }
            Act::ExportPdf => {
                if let Some(path) = widgets::report(status, db.export_euer_pdf(self.year), "") {
                    *status = format!("PDF gespeichert: {}", path.display());
                    widgets::open_file(&path, status);
                }
            }
        }
    }
}
