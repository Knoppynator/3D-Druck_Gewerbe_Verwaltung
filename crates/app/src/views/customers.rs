use druckplaner_core::{catalog::Customer, Db};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct Customers {
    items: Vec<Customer>,
    form: Customer,
}

impl Customers {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.list_customers(), "") {
            self.items = items;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        egui::SidePanel::left("customers_list").default_width(240.0).show_inside(ui, |ui| {
            if ui.button("Neuer Kunde").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for c in &self.items {
                    if ui.selectable_label(self.form.id == c.id, c.company.as_str()).clicked() {
                        act = Action::Select(c.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(if self.form.id == 0 { "Neuer Kunde" } else { "Kunde bearbeiten" });
            ui.add_space(8.0);
            egui::Grid::new("customer_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                widgets::text_row(ui, "Firma / Name", &mut self.form.company);
                widgets::text_row(ui, "Ansprechpartner", &mut self.form.contact);
                widgets::text_row(ui, "Anrede (Brief)", &mut self.form.salutation);
                widgets::text_row(ui, "Straße, Hausnr.", &mut self.form.street);
                widgets::text_row(ui, "PLZ", &mut self.form.zip);
                widgets::text_row(ui, "Ort", &mut self.form.city);
                widgets::text_row(ui, "Notiz", &mut self.form.note);
            });
            ui.add_space(8.0);
            widgets::form_buttons(ui, self.form.id != 0, &mut act);
        });

        match act {
            Action::None => {}
            Action::New => self.form = Customer::default(),
            Action::Select(id) => {
                if let Some(c) = self.items.iter().find(|c| c.id == id) {
                    self.form = c.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_customer(&mut self.form), "Kunde gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_customer(self.form.id), "Kunde gelöscht").is_some() {
                    self.form = Customer::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
