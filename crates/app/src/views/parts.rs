use druckplaner_core::{
    catalog::{PrintPart, PurchasePart},
    Db,
};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct PrintParts {
    items: Vec<PrintPart>,
    form: PrintPart,
}

impl PrintParts {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.list_print_parts(), "") {
            self.items = items;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        let rows: Vec<(i64, String, String)> = self
            .items
            .iter()
            .map(|p| (p.id, p.name.clone(), p.notes.lines().next().unwrap_or("").to_string()))
            .collect();
        let subtitle = format!("{} Teil(e) – was auf den Platten gedruckt wird", self.items.len());
        widgets::master_list(ui, ("Druckteile", &subtitle, "Neues Druckteil"), "print_parts_list", &rows, self.form.id, &mut act);

        widgets::detail_panel(ui, |ui| {
            let title = if self.form.id == 0 { "Neues Druckteil" } else { "Druckteil bearbeiten" };
            widgets::card(ui, title, "", |ui| {
                egui::Grid::new("print_part_form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    widgets::text_row(ui, "Name", &mut self.form.name);
                    widgets::form_label(ui, "Druckhinweise");
                    ui.add(egui::TextEdit::multiline(&mut self.form.notes).desired_width(320.0).desired_rows(4));
                    ui.end_row();
                });
                ui.add_space(8.0);
                widgets::form_buttons(ui, self.form.id != 0, &mut act);
            });
        });

        match act {
            Action::None => {}
            Action::New => self.form = PrintPart::default(),
            Action::Select(id) => {
                if let Some(p) = self.items.iter().find(|p| p.id == id) {
                    self.form = p.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_print_part(&mut self.form), "Druckteil gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_print_part(self.form.id), "Druckteil gelöscht").is_some() {
                    self.form = PrintPart::default();
                    self.reload(db, status);
                }
            }
        }
    }
}

#[derive(Default)]
pub struct PurchaseParts {
    items: Vec<PurchasePart>,
    form: PurchasePart,
}

impl PurchaseParts {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.list_purchase_parts(), "") {
            self.items = items;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        let rows: Vec<(i64, String, String)> = self
            .items
            .iter()
            .map(|p| {
                let sale = if p.sellable { " · verkaufbar" } else { "" };
                (p.id, p.name.clone(), format!("{} € je {}{sale}", widgets::dec(p.unit_cost_eur, 3), p.unit))
            })
            .collect();
        let subtitle = format!("{} Teil(e) – Schrauben, Saugnäpfe und alles, was eingekauft wird", self.items.len());
        widgets::master_list(ui, ("Zukaufteile", &subtitle, "Neues Zukaufteil"), "purchase_parts_list", &rows, self.form.id, &mut act);

        widgets::detail_panel(ui, |ui| {
            let title = if self.form.id == 0 { "Neues Zukaufteil" } else { "Zukaufteil bearbeiten" };
            widgets::card(ui, title, "", |ui| {
                egui::Grid::new("purchase_part_form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    widgets::text_row(ui, "Name", &mut self.form.name);
                    widgets::text_row(ui, "Einheit", &mut self.form.unit);
                    widgets::form_label(ui, "Einkaufspreis je Einheit");
                    widgets::dec_drag(ui, &mut self.form.unit_cost_eur, 0.0..=100_000.0, 0.001, 3, " €");
                    ui.end_row();
                    widgets::int_row(ui, "Mindestbestand", &mut self.form.min_stock, "");
                    widgets::form_label(ui, "Einzeln verkaufbar");
                    ui.checkbox(&mut self.form.sellable, "");
                    ui.end_row();
                    if self.form.sellable {
                        widgets::eur_row(ui, "Verkaufspreis je Einheit", &mut self.form.sale_price_cents);
                    }
                });
                ui.add_space(8.0);
                widgets::form_buttons(ui, self.form.id != 0, &mut act);
            });
        });

        match act {
            Action::None => {}
            Action::New => self.form = PurchasePart::default(),
            Action::Select(id) => {
                if let Some(p) = self.items.iter().find(|p| p.id == id) {
                    self.form = p.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_purchase_part(&mut self.form), "Zukaufteil gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_purchase_part(self.form.id), "Zukaufteil gelöscht").is_some() {
                    self.form = PurchasePart::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
