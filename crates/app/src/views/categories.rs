use druckplaner_core::{finance::Category, Db};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct Categories {
    items: Vec<Category>,
    form: Category,
}

impl Categories {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.list_categories(), "") {
            self.items = items;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        egui::SidePanel::left("categories_list").default_width(260.0).show_inside(ui, |ui| {
            if ui.button("Neue Kategorie").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for c in &self.items {
                    if ui.selectable_label(self.form.id == c.id, c.name.as_str()).clicked() {
                        act = Action::Select(c.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading(if self.form.id == 0 { "Neue Ausgaben-Kategorie" } else { "Kategorie bearbeiten" });
            ui.add_space(8.0);
            egui::Grid::new("category_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                widgets::text_row(ui, "Name", &mut self.form.name);
                widgets::text_row(ui, "EÜR-Zeile", &mut self.form.euer_line);
                widgets::text_row(ui, "EÜR-Bezeichnung", &mut self.form.euer_label);
            });
            ui.add_space(4.0);
            ui.weak("Die Zeilennummern ändern sich von Jahr zu Jahr – bitte anhand des aktuellen ELSTER-Formulars eintragen.");
            ui.add_space(8.0);
            widgets::form_buttons(ui, self.form.id != 0, &mut act);
        });

        match act {
            Action::None => {}
            Action::New => self.form = Category::default(),
            Action::Select(id) => {
                if let Some(c) = self.items.iter().find(|c| c.id == id) {
                    self.form = c.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_category(&mut self.form), "Kategorie gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_category(self.form.id), "Kategorie gelöscht").is_some() {
                    self.form = Category::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
