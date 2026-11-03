use druckplaner_core::{catalog::Material, Db};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct Materials {
    items: Vec<Material>,
    form: Material,
}

impl Materials {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.list_materials(), "") {
            self.items = items;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        egui::SidePanel::left("materials_list").default_width(240.0).show_inside(ui, |ui| {
            if ui.button("Neues Material").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for m in &self.items {
                    if ui.selectable_label(self.form.id == m.id, m.label()).clicked() {
                        act = Action::Select(m.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(if self.form.id == 0 { "Neues Material" } else { "Material bearbeiten" });
            ui.add_space(8.0);
            egui::Grid::new("material_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                widgets::text_row(ui, "Werkstoff (z. B. PETG)", &mut self.form.kind);
                widgets::text_row(ui, "Farbe", &mut self.form.color);
                widgets::text_row(ui, "Hersteller", &mut self.form.vendor);
                widgets::eur_row(ui, "Preis pro kg", &mut self.form.price_per_kg_cents);
                widgets::int_row(ui, "Mindestbestand", &mut self.form.min_stock_g, " g");
                widgets::int_row(ui, "Lieferzeit", &mut self.form.lead_time_days, " Tage");
            });
            ui.add_space(8.0);
            widgets::form_buttons(ui, self.form.id != 0, &mut act);
        });

        match act {
            Action::None => {}
            Action::New => self.form = Material::default(),
            Action::Select(id) => {
                if let Some(m) = self.items.iter().find(|m| m.id == id) {
                    self.form = m.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_material(&mut self.form), "Material gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_material(self.form.id), "Material gelöscht").is_some() {
                    self.form = Material::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
