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

        let rows: Vec<(i64, String, String)> = self
            .items
            .iter()
            .map(|c| {
                let line = if c.euer_line.is_empty() { String::new() } else { format!("Zeile {} · ", c.euer_line) };
                (c.id, c.name.clone(), format!("{line}{}", c.euer_label))
            })
            .collect();
        let subtitle = format!("{} Kategorie(n) für Ausgaben und ihre Zeile in der EÜR", self.items.len());
        widgets::master_list(ui, ("Kategorien", &subtitle, "Neue Kategorie"), "categories_list", &rows, self.form.id, &mut act);

        widgets::detail_panel(ui, |ui| {
            let title = if self.form.id == 0 { "Neue Ausgaben-Kategorie" } else { "Kategorie bearbeiten" };
            widgets::card(ui, title, "", |ui| {
                egui::Grid::new("category_form").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    widgets::text_row(ui, "Name", &mut self.form.name);
                    widgets::text_row(ui, "EÜR-Zeile", &mut self.form.euer_line);
                    widgets::text_row(ui, "EÜR-Bezeichnung", &mut self.form.euer_label);
                });
                widgets::hint(ui, "Die Zeilennummern ändern sich von Jahr zu Jahr – bitte anhand des aktuellen ELSTER-Formulars eintragen.");
                ui.add_space(8.0);
                widgets::form_buttons(ui, self.form.id != 0, &mut act);
            });
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
