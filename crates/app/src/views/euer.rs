use druckplaner_core::{
    date::format_de,
    finance::EuerReport,
    money::format_eur,
    Db,
};
use eframe::egui;

use crate::{
    theme,
    widgets::{self, Stat, Tone},
};

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
        let c = theme::p();

        egui::ScrollArea::vertical().show(ui, |ui| {
            widgets::page_header(ui, "Einnahmen-Überschuss-Rechnung", "Einnahmen nach Zahlungsdatum, Ausgaben je Kategorie und EÜR-Zeile", |ui| {
                if ui.button("CSV exportieren").clicked() {
                    act = Act::ExportCsv;
                }
                if widgets::primary_button(ui, "PDF exportieren").clicked() {
                    act = Act::ExportPdf;
                }
                egui::ComboBox::from_id_salt("euer_year").width(90.0).selected_text(self.year.to_string()).show_ui(ui, |ui| {
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
                widgets::form_label(ui, "Jahr");
            });

            let profit_tone = if r.profit_cents < 0 { Tone::Critical } else { Tone::Good };
            let stats = [
                Stat::new(format!("Betriebseinnahmen {}", self.year), format_eur(r.income_total_cents), format!("{} Zahlung(en)", r.income.len())),
                Stat::new("Betriebsausgaben", format_eur(r.expense_total_cents), format!("{} Buchung(en)", r.expenses.len())),
                Stat::new("Gewinn / Verlust", format_eur(r.profit_cents), if r.profit_cents < 0 { "Verlust" } else { "Gewinn" }).tone(profit_tone),
                if r.missing_receipts > 0 {
                    Stat::new("Belege", format!("{} fehlen", r.missing_receipts), "bitte nachreichen").tone(Tone::Warn)
                } else {
                    Stat::new("Belege", "vollständig", "zu jeder Ausgabe liegt ein Beleg vor")
                },
            ];
            widgets::stat_rows(ui, &stats, 200.0);

            widgets::card(ui, "Anlage EÜR", "Zeilen unter Stammdaten › Kategorien und in den Einstellungen pflegen", |ui| {
                egui::Grid::new("euer_summary").striped(true).spacing([20.0, 7.0]).show(ui, |ui| {
                    widgets::table_head(ui, &["Zeile", "Position", "Betrag"]);
                    ui.label(r.income_line.as_str());
                    ui.label(r.income_label.as_str());
                    ui.label(format_eur(r.income_total_cents));
                    ui.end_row();
                    for cat in &r.by_category {
                        ui.label(cat.euer_line.as_str());
                        let label = if cat.euer_label.is_empty() { &cat.category } else { &cat.euer_label };
                        ui.horizontal(|ui| {
                            ui.label(label.as_str());
                            widgets::hint(ui, &format!("{}, {} Buchungen", cat.category, cat.count));
                        });
                        ui.label(format_eur(-cat.total_cents));
                        ui.end_row();
                    }
                    ui.label("");
                    ui.label(theme::strong("Summe Betriebsausgaben", 14.0));
                    ui.label(theme::strong(format_eur(r.expense_total_cents), 14.0));
                    ui.end_row();
                    ui.label("");
                    ui.label(theme::strong("Gewinn / Verlust", 15.0));
                    let color = if r.profit_cents < 0 { c.critical_text } else { c.text };
                    ui.label(egui::RichText::new(format_eur(r.profit_cents)).family(theme::semibold()).size(15.0).color(color));
                    ui.end_row();
                });
            });

            widgets::two_up(
                ui,
                1000.0,
                |ui| {
                    widgets::card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let title = egui::RichText::new(format!("Einnahmen ({})", r.income.len())).family(theme::semibold()).size(15.0);
                        egui::CollapsingHeader::new(title).id_salt("euer_income_header").default_open(false).show(ui, |ui| {
                            egui::Grid::new("euer_income").striped(true).spacing([16.0, 5.0]).show(ui, |ui| {
                                for i in &r.income {
                                    ui.label(format_de(&i.paid_at));
                                    ui.label(i.invoice_number.as_str());
                                    ui.label(i.customer.as_str());
                                    ui.label(format_eur(i.amount_cents));
                                    ui.end_row();
                                }
                            });
                        });
                    });
                },
                |ui| {
                    widgets::card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let title = egui::RichText::new(format!("Ausgaben ({})", r.expenses.len())).family(theme::semibold()).size(15.0);
                        egui::CollapsingHeader::new(title).id_salt("euer_expenses_header").default_open(false).show(ui, |ui| {
                            egui::Grid::new("euer_expenses").striped(true).spacing([16.0, 5.0]).show(ui, |ui| {
                                for e in &r.expenses {
                                    let x = &e.expense;
                                    ui.label(format_de(&x.spent_at));
                                    ui.label(x.description.as_str());
                                    ui.label(format_eur(x.amount_cents));
                                    if x.has_receipt() {
                                        widgets::hint(ui, x.receipt_label());
                                    } else {
                                        widgets::chip(ui, Tone::Warn, "Beleg fehlt");
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                    });
                },
            );
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
