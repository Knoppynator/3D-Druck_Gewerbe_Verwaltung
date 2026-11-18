use druckplaner_core::{catalog::Material, money::format_eur, Db};
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

        let rows: Vec<(i64, String, String)> = self
            .items
            .iter()
            .map(|m| (m.id, m.label(), format!("{} je kg{}", format_eur(m.price_per_kg_cents), if m.vendor.is_empty() { String::new() } else { format!(" · {}", m.vendor) })))
            .collect();
        let subtitle = format!("{} Sorte(n) – Werkstoff und Farbe, Preis, Mindestbestand und Lieferzeit", self.items.len());
        widgets::master_list(ui, ("Filament", &subtitle, "Neues Filament"), "materials_list", &rows, self.form.id, &mut act);

        widgets::detail_panel(ui, |ui| {
            let title = if self.form.id == 0 { "Neues Filament" } else { "Filament bearbeiten" };
            widgets::card(ui, title, "", |ui| {
                egui::Grid::new("material_form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
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
