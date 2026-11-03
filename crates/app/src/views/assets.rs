use druckplaner_core::{
    assets::{Asset, AssetMethod, GWG_LIMIT_CENTS},
    date::format_de,
    finance::ExpenseRow,
    money::format_eur,
    Db,
};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct Assets {
    items: Vec<Asset>,
    form: Asset,
    expenses: Vec<ExpenseRow>,
}

impl Assets {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_assets(), "") {
            self.items = v;
        }
        if let Some(v) = widgets::report(status, db.list_expenses(None), "") {
            self.expenses = v;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;
        let error = ui.visuals().error_fg_color;

        egui::SidePanel::left("assets_list").default_width(300.0).show_inside(ui, |ui| {
            if ui.button("Neue Anlage").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for a in &self.items {
                    let text = format!("{}\n{} · {} · {}", a.name, format_de(&a.acquired_at), format_eur(a.cost_cents), a.method.label());
                    if ui.selectable_label(self.form.id == a.id, text).clicked() {
                        act = Action::Select(a.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(if self.form.id == 0 { "Neue Anlage" } else { "Anlage bearbeiten" });
                ui.add_space(8.0);
                let before_expense = self.form.expense_id;
                egui::Grid::new("asset_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    widgets::text_row(ui, "Bezeichnung", &mut self.form.name);
                    widgets::date_row(ui, "Anschaffung", &mut self.form.acquired_at);
                    widgets::eur_row(ui, "Anschaffungskosten", &mut self.form.cost_cents);
                    ui.label("Abschreibung");
                    egui::ComboBox::from_id_salt("asset_method")
                        .selected_text(self.form.method.label())
                        .show_ui(ui, |ui| {
                            for m in [AssetMethod::Gwg, AssetMethod::Linear] {
                                ui.selectable_value(&mut self.form.method, m, m.label());
                            }
                        });
                    ui.end_row();
                    if self.form.method == AssetMethod::Linear {
                        widgets::int_row(ui, "Nutzungsdauer", &mut self.form.useful_life_years, " Jahre");
                    }
                    ui.label("Zugehörige Ausgabe");
                    egui::ComboBox::from_id_salt("asset_expense")
                        .width(360.0)
                        .selected_text(
                            self.expenses
                                .iter()
                                .find(|e| Some(e.expense.id) == self.form.expense_id)
                                .map_or("(keine)".to_string(), |e| {
                                    format!("{} · {} · {}", format_de(&e.expense.spent_at), e.expense.description, format_eur(e.expense.amount_cents))
                                }),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.form.expense_id, None, "(keine)");
                            for e in &self.expenses {
                                ui.selectable_value(
                                    &mut self.form.expense_id,
                                    Some(e.expense.id),
                                    format!("{} · {} · {}", format_de(&e.expense.spent_at), e.expense.description, format_eur(e.expense.amount_cents)),
                                );
                            }
                        });
                    ui.end_row();
                    widgets::text_row(ui, "Notiz", &mut self.form.note);
                });
                // Beim Verknüpfen einer Ausgabe leere Felder daraus vorbelegen
                if self.form.expense_id != before_expense {
                    if let Some(e) = self.expenses.iter().find(|e| Some(e.expense.id) == self.form.expense_id) {
                        if self.form.name.trim().is_empty() {
                            self.form.name = e.expense.description.clone();
                        }
                        if self.form.acquired_at.is_empty() {
                            self.form.acquired_at = e.expense.spent_at.clone();
                        }
                        if self.form.cost_cents == 0 {
                            self.form.cost_cents = e.expense.amount_cents.abs();
                        }
                    }
                }

                ui.add_space(6.0);
                if self.form.method == AssetMethod::Gwg && self.form.cost_cents > GWG_LIMIT_CENTS {
                    ui.colored_label(error, "Über 800 € ist ein GWG nicht möglich – bitte linear abschreiben.");
                } else if self.form.method == AssetMethod::Gwg {
                    ui.weak("GWG: sofort abziehbar. Die zugehörige Ausgabe bleibt als normale Betriebsausgabe in der EÜR.");
                } else {
                    ui.weak("Linear: Die zugehörige Ausgabe zählt nicht mehr als Betriebsausgabe, stattdessen die AfA je Jahr (monatsgenau ab Anschaffung).");
                    if self.form.expense_id.is_none() {
                        ui.colored_label(error, "Ohne zugehörige Ausgabe werden Anschaffungskosten und AfA doppelt gezählt, sobald die Ausgabe separat erfasst ist.");
                    }
                    let schedule = self.form.schedule();
                    if !schedule.is_empty() {
                        ui.add_space(4.0);
                        ui.strong("Abschreibung je Jahr");
                        egui::Grid::new("asset_schedule").spacing([16.0, 3.0]).show(ui, |ui| {
                            for (year, cents) in schedule {
                                ui.label(year.to_string());
                                ui.label(format_eur(cents));
                                ui.end_row();
                            }
                        });
                    }
                }
                ui.add_space(8.0);
                widgets::form_buttons(ui, self.form.id != 0, &mut act);
            });
        });

        match act {
            Action::None => {}
            Action::New => self.form = Asset::default(),
            Action::Select(id) => {
                if let Some(a) = self.items.iter().find(|a| a.id == id) {
                    self.form = a.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_asset(&mut self.form), "Anlage gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_asset(self.form.id), "Anlage gelöscht").is_some() {
                    self.form = Asset::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
