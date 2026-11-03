use druckplaner_core::{
    catalog::{Material, PrintPart},
    date::format_de,
    stock::{reason_label, StockKey, StockLevel, StockMove},
    Db,
};
use eframe::egui;

use crate::widgets;

enum Act {
    None,
    Select(StockKey),
    Add,
    Scrap,
    Inventory,
    SetMin,
    DeleteMove(i64),
}

#[derive(Default)]
pub struct StockView {
    levels: Vec<StockLevel>,
    selected: Option<StockKey>,
    /// Name und Einheit des gewählten Postens (auch wenn er noch keine Bewegungen hat)
    selected_label: (String, String),
    moves: Vec<StockMove>,
    amount: i64,
    counted: i64,
    /// Mindestbestand fertiger Teile (nur bei Fertigteil-Posten)
    min_edit: i64,
    note: String,
    today: String,
    print_parts: Vec<PrintPart>,
    materials: Vec<Material>,
    new_part: i64,
    new_material: i64,
}

impl StockView {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.stock_levels(), "") {
            self.levels = v;
        }
        if let Some(v) = widgets::report(status, db.today(), "") {
            self.today = v;
        }
        if let Some(v) = widgets::report(status, db.list_print_parts(), "") {
            self.print_parts = v;
        }
        if let Some(v) = widgets::report(status, db.list_materials(), "") {
            self.materials = v;
        }
        if let Some(key) = self.selected {
            self.select(db, status, key);
        }
    }

    pub fn select(&mut self, db: &Db, status: &mut String, key: StockKey) {
        let Some(label) = widgets::report(status, db.stock_label(key), "") else { return };
        self.selected_label = label;
        self.moves = widgets::report(status, db.stock_moves(key, 60), "").unwrap_or_default();
        self.amount = 0;
        self.counted = self.levels.iter().find(|l| l.key == key).map_or(0, |l| l.qty.max(0));
        self.min_edit = self.levels.iter().find(|l| l.key == key).map_or(0, |l| l.min_qty);
        self.selected = Some(key);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;
        let error = ui.visuals().error_fg_color;

        egui::SidePanel::left("stock_list").default_width(330.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (title, filter) in [
                    ("Filament", 0),
                    ("Fertige Druckteile", 1),
                    ("Zukaufteile", 2),
                ] {
                    ui.strong(title);
                    let mut any = false;
                    for l in self.levels.iter().filter(|l| {
                        matches!(
                            (filter, l.key),
                            (0, StockKey::Filament { .. }) | (1, StockKey::PrintPart { .. }) | (2, StockKey::PurchasePart { .. })
                        )
                    }) {
                        any = true;
                        let mut text = egui::RichText::new(format!("{}   {} {}", l.name, l.qty, l.unit));
                        if l.below_min() || l.qty < 0 {
                            text = text.color(error);
                        }
                        if ui.selectable_label(self.selected == Some(l.key), text).clicked() {
                            act = Act::Select(l.key);
                        }
                    }
                    if !any {
                        ui.weak("noch keine Posten");
                    }
                    ui.add_space(8.0);
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.detail_ui(ui, &mut act, error));
        });

        self.handle(act, db, status);
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, act: &mut Act, error: egui::Color32) {
        let Some(key) = self.selected else {
            ui.heading("Lager");
            ui.label("Posten links auswählen. Der Bestand ist die Summe aller Bewegungen: Wareneingang (bei den Ausgaben buchen), Lieferungen und Druckjobs buchen automatisch aus.");
            ui.add_space(12.0);
            self.finished_part_picker(ui, act);
            return;
        };
        let (name, unit) = self.selected_label.clone();
        let level = self.levels.iter().find(|l| l.key == key).cloned();
        ui.heading(&name);
        let qty = level.as_ref().map_or(0, |l| l.qty);
        ui.label(egui::RichText::new(format!("Bestand: {qty} {unit}")).size(16.0).strong());
        if let Some(l) = &level {
            if l.min_qty > 0 {
                let text = format!("Mindestbestand {} {unit}", l.min_qty);
                if l.below_min() {
                    ui.colored_label(error, format!("{text} – unterschritten, nachbestellen"));
                } else {
                    ui.label(text);
                }
            }
            if l.consumed_90d > 0 {
                let range = l.days_left().map_or(String::new(), |d| format!(" – reicht noch ca. {d} Tage"));
                ui.label(format!("Verbrauch letzte 90 Tage: {} {unit}{range}", l.consumed_90d));
                if l.lead_time_days > 0 {
                    ui.weak(format!("Lieferzeit {} Tage", l.lead_time_days));
                }
            }
        }
        if qty < 0 {
            ui.colored_label(error, "Negativer Bestand – wahrscheinlich fehlt der Anfangsbestand. Mit einer Inventur korrigieren.");
        }

        ui.add_space(10.0);
        ui.strong("Buchen");
        ui.horizontal(|ui| {
            ui.label("Menge");
            ui.add(egui::DragValue::new(&mut self.amount).range(0..=10_000_000).suffix(format!(" {unit}")));
            ui.add(egui::TextEdit::singleline(&mut self.note).desired_width(200.0).hint_text("Notiz"));
            if ui.button("Zugang").clicked() {
                *act = Act::Add;
            }
            if ui.button("Ausschuss / Abgang").clicked() {
                *act = Act::Scrap;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Gezählt");
            ui.add(egui::DragValue::new(&mut self.counted).range(0..=10_000_000).suffix(format!(" {unit}")));
            if ui.button("Inventur buchen").on_hover_text("Setzt den Bestand auf die gezählte Menge").clicked() {
                *act = Act::Inventory;
            }
        });
        if matches!(key, StockKey::PrintPart { .. }) {
            ui.horizontal(|ui| {
                ui.label("Mindestbestand");
                ui.add(egui::DragValue::new(&mut self.min_edit).range(0..=100_000).suffix(format!(" {unit}")));
                if ui.button("Speichern").on_hover_text("Die Produktion füllt den Bestand bei freier Druckerzeit auf diesen Wert auf (0 = kein Mindestbestand)").clicked() {
                    *act = Act::SetMin;
                }
            });
        }
        if matches!(key, StockKey::Filament { .. } | StockKey::PurchasePart { .. }) {
            ui.weak("Wareneingang aus einer Einkaufsrechnung buchst du bei der Ausgabe („Wareneingang“), dann wird auch der Einkaufspreis aktualisiert.");
        }

        ui.add_space(10.0);
        ui.strong("Bewegungen");
        egui::Grid::new("stock_moves").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for m in &self.moves {
                ui.label(format_de(&m.moved_at));
                ui.label(reason_label(&m.reason));
                let sign = if m.qty > 0 { "+" } else { "" };
                ui.label(format!("{sign}{} {unit}", m.qty));
                ui.label(m.note.as_str());
                if m.is_manual() {
                    if ui.small_button("×").on_hover_text("Bewegung löschen").clicked() {
                        *act = Act::DeleteMove(m.id);
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });

        ui.add_space(16.0);
        ui.separator();
        self.finished_part_picker(ui, act);
    }

    /// Fertigteil-Posten (Druckteil + Farbe) neu anlegen, bevor die erste Bewegung gebucht wird.
    fn finished_part_picker(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        ui.strong("Fertigteil-Posten anlegen");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("new_part")
                .width(160.0)
                .selected_text(self.print_parts.iter().find(|p| p.id == self.new_part).map_or("Druckteil", |p| p.name.as_str()))
                .show_ui(ui, |ui| {
                    for p in &self.print_parts {
                        ui.selectable_value(&mut self.new_part, p.id, p.name.as_str());
                    }
                });
            egui::ComboBox::from_id_salt("new_material")
                .width(160.0)
                .selected_text(self.materials.iter().find(|m| m.id == self.new_material).map_or("Material".to_string(), Material::label))
                .show_ui(ui, |ui| {
                    for m in &self.materials {
                        ui.selectable_value(&mut self.new_material, m.id, m.label());
                    }
                });
            if ui.add_enabled(self.new_part != 0 && self.new_material != 0, egui::Button::new("Auswählen")).clicked() {
                *act = Act::Select(StockKey::PrintPart { print_part_id: self.new_part, material_id: self.new_material });
            }
        });
    }

    fn handle(&mut self, act: Act, db: &Db, status: &mut String) {
        let Some(key) = self.selected else {
            if let Act::Select(key) = act {
                self.select(db, status, key);
            }
            return;
        };
        let note = self.note.trim().to_string();
        match act {
            Act::None => {}
            Act::Select(key) => self.select(db, status, key),
            Act::Add | Act::Scrap => {
                let (qty, reason) = if matches!(act, Act::Add) { (self.amount, "manual") } else { (-self.amount, "scrap") };
                let result = db.add_stock_move(key, qty, &self.today, reason, ("", 0), &note);
                if widgets::report(status, result, "Bewegung gebucht").is_some() {
                    self.note.clear();
                    self.reload(db, status);
                }
            }
            Act::Inventory => {
                let result = db.stock_inventory(key, self.counted, &self.today, &note);
                if let Some(delta) = widgets::report(status, result, "") {
                    *status = if delta == 0 { "Bestand stimmt bereits".to_string() } else { format!("Inventur gebucht (Differenz {delta:+})") };
                    self.note.clear();
                    self.reload(db, status);
                }
            }
            Act::SetMin => {
                if widgets::report(status, db.set_finished_min(key, self.min_edit), "Mindestbestand gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Act::DeleteMove(id) => {
                if widgets::report(status, db.delete_stock_move(id), "Bewegung gelöscht").is_some() {
                    self.reload(db, status);
                }
            }
        }
    }
}
