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

        egui::SidePanel::left("print_parts_list").default_width(240.0).show_inside(ui, |ui| {
            if ui.button("Neues Druckteil").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &self.items {
                    if ui.selectable_label(self.form.id == p.id, p.name.as_str()).clicked() {
                        act = Action::Select(p.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(if self.form.id == 0 { "Neues Druckteil" } else { "Druckteil bearbeiten" });
            ui.add_space(8.0);
            egui::Grid::new("print_part_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                widgets::text_row(ui, "Name", &mut self.form.name);
                ui.label("Druckhinweise");
                ui.add(egui::TextEdit::multiline(&mut self.form.notes).desired_width(320.0).desired_rows(4));
                ui.end_row();
            });
            ui.add_space(8.0);
            widgets::form_buttons(ui, self.form.id != 0, &mut act);
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

        egui::SidePanel::left("purchase_parts_list").default_width(240.0).show_inside(ui, |ui| {
            if ui.button("Neues Zukaufteil").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &self.items {
                    if ui.selectable_label(self.form.id == p.id, p.name.as_str()).clicked() {
                        act = Action::Select(p.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(if self.form.id == 0 { "Neues Zukaufteil" } else { "Zukaufteil bearbeiten" });
            ui.add_space(8.0);
            egui::Grid::new("purchase_part_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                widgets::text_row(ui, "Name", &mut self.form.name);
                widgets::text_row(ui, "Einheit", &mut self.form.unit);
                ui.label("Einkaufspreis je Einheit");
                widgets::dec_drag(ui, &mut self.form.unit_cost_eur, 0.0..=100_000.0, 0.001, 3, " €");
                ui.end_row();
                widgets::int_row(ui, "Mindestbestand", &mut self.form.min_stock, "");
                ui.label("Einzeln verkaufbar");
                ui.checkbox(&mut self.form.sellable, "");
                ui.end_row();
                if self.form.sellable {
                    widgets::eur_row(ui, "Verkaufspreis je Einheit", &mut self.form.sale_price_cents);
                }
            });
            ui.add_space(8.0);
            widgets::form_buttons(ui, self.form.id != 0, &mut act);
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
