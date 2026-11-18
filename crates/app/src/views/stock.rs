use druckplaner_core::{
    catalog::{Material, PrintPart},
    date::format_de,
    stock::{reason_label, StockKey, StockLevel, StockMove},
    Db,
};
use eframe::egui;

use crate::{
    theme,
    widgets::{self, Tone},
};

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

        let low = self.levels.iter().filter(|l| l.below_min() || l.qty < 0).count();
        let subtitle = format!(
            "{} Posten · {low} unter Mindestbestand · Bestand = Summe aller Bewegungen (Lieferungen und Druckjobs buchen automatisch)",
            self.levels.len()
        );
        widgets::page_header(ui, "Lager", &subtitle, |_| {});

        widgets::list_panel(ui, "stock_list", |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (title, filter) in [("FILAMENT", 0), ("FERTIGE DRUCKTEILE", 1), ("ZUKAUFTEILE", 2)] {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(title).size(11.0).family(theme::semibold()).color(theme::p().muted));
                    let mut any = false;
                    for l in self.levels.iter().filter(|l| {
                        matches!(
                            (filter, l.key),
                            (0, StockKey::Filament { .. }) | (1, StockKey::PrintPart { .. }) | (2, StockKey::PurchasePart { .. })
                        )
                    }) {
                        any = true;
                        let qty = format!("{} {}", l.qty, l.unit);
                        let mut entry = widgets::ListEntry::new(&l.name);
                        entry.right = &qty;
                        if l.qty < 0 {
                            entry.chip = Some((Tone::Critical, "negativ – Inventur fehlt"));
                        } else if l.below_min() {
                            entry.chip = Some((Tone::Critical, "unter Mindestbestand"));
                        }
                        if widgets::list_entry(ui, ("stock", format!("{:?}", l.key)), self.selected == Some(l.key), &entry).clicked() {
                            act = Act::Select(l.key);
                        }
                    }
                    if !any {
                        widgets::hint(ui, "noch keine Posten");
                    }
                    ui.add_space(6.0);
                }
            });
        });

        widgets::detail_panel(ui, |ui| self.detail_ui(ui, &mut act));

        self.handle(act, db, status);
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let c = theme::p();
        let Some(key) = self.selected else {
            widgets::card(ui, "Kein Posten gewählt", "", |ui| {
                widgets::hint(
                    ui,
                    "Posten links auswählen. Der Bestand ist die Summe aller Bewegungen: Wareneingang (bei den Ausgaben buchen), \
                     Lieferungen und Druckjobs buchen automatisch aus.",
                );
            });
            self.finished_part_picker(ui, act);
            return;
        };
        let (name, unit) = self.selected_label.clone();
        let level = self.levels.iter().find(|l| l.key == key).cloned();
        let qty = level.as_ref().map_or(0, |l| l.qty);

        // Kopfkarte: Bestand groß, darunter Zustand
        widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::hint(ui, &name);
            ui.horizontal(|ui| {
                ui.label(theme::strong(format!("{qty} {unit}"), 30.0));
                if qty < 0 {
                    widgets::chip(ui, Tone::Critical, "negativer Bestand");
                } else if level.as_ref().is_some_and(|l| l.below_min()) {
                    widgets::chip(ui, Tone::Critical, "unter Mindestbestand");
                } else {
                    widgets::chip(ui, Tone::Good, "im Soll");
                }
            });
            if let Some(l) = &level {
                let mut facts = Vec::new();
                if l.min_qty > 0 {
                    facts.push(format!("Mindestbestand {} {unit}", l.min_qty));
                }
                if l.consumed_90d > 0 {
                    facts.push(format!("Verbrauch 90 Tage {} {unit}", l.consumed_90d));
                    if let Some(d) = l.days_left() {
                        facts.push(format!("reicht noch ca. {d} Tage"));
                    }
                }
                if l.lead_time_days > 0 {
                    facts.push(format!("Lieferzeit {} Tage", l.lead_time_days));
                }
                if !facts.is_empty() {
                    ui.label(egui::RichText::new(facts.join(" · ")).color(c.text_soft));
                }
            }
            if qty < 0 {
                ui.colored_label(c.critical_text, "Wahrscheinlich fehlt der Anfangsbestand – mit einer Inventur korrigieren.");
            }
        });
        ui.add_space(4.0);

        widgets::card(ui, "Buchen", "", |ui| {
            egui::Grid::new("stock_book").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                widgets::form_label(ui, "Menge");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.amount).range(0..=10_000_000).suffix(format!(" {unit}")));
                    ui.add(egui::TextEdit::singleline(&mut self.note).desired_width(200.0).hint_text("Notiz"));
                    if widgets::soft_button(ui, "Zugang").clicked() {
                        *act = Act::Add;
                    }
                    if widgets::danger_button(ui, "Ausschuss / Abgang").clicked() {
                        *act = Act::Scrap;
                    }
                });
                ui.end_row();
                widgets::form_label(ui, "Gezählt");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.counted).range(0..=10_000_000).suffix(format!(" {unit}")));
                    if ui.button("Inventur buchen").on_hover_text("Setzt den Bestand auf die gezählte Menge").clicked() {
                        *act = Act::Inventory;
                    }
                });
                ui.end_row();
                if matches!(key, StockKey::PrintPart { .. }) {
                    widgets::form_label(ui, "Mindestbestand");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.min_edit).range(0..=100_000).suffix(format!(" {unit}")));
                        if ui
                            .button("Speichern")
                            .on_hover_text("Die Produktion füllt den Bestand bei freier Druckerzeit auf diesen Wert auf (0 = kein Mindestbestand)")
                            .clicked()
                        {
                            *act = Act::SetMin;
                        }
                    });
                    ui.end_row();
                }
            });
            if matches!(key, StockKey::Filament { .. } | StockKey::PurchasePart { .. }) {
                widgets::hint(ui, "Wareneingang aus einer Einkaufsrechnung buchst du bei der Ausgabe („Wareneingang“), dann wird auch der Einkaufspreis aktualisiert.");
            }
        });

        widgets::card(ui, "Bewegungen", "die letzten 60", |ui| {
            if self.moves.is_empty() {
                widgets::hint(ui, "Noch keine Bewegungen.");
            }
            egui::Grid::new("stock_moves").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                for m in &self.moves {
                    ui.label(format_de(&m.moved_at));
                    ui.label(reason_label(&m.reason));
                    let (sign, color) = if m.qty > 0 { ("+", c.good_text) } else { ("", c.text) };
                    ui.label(egui::RichText::new(format!("{sign}{} {unit}", m.qty)).family(theme::semibold()).color(color));
                    widgets::hint(ui, m.note.as_str());
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
        });

        self.finished_part_picker(ui, act);
    }

    /// Fertigteil-Posten (Druckteil + Farbe) neu anlegen, bevor die erste Bewegung gebucht wird.
    fn finished_part_picker(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        widgets::card(ui, "Fertigteil-Posten anlegen", "Druckteil in einer Farbe", |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("new_part")
                    .width(180.0)
                    .selected_text(self.print_parts.iter().find(|p| p.id == self.new_part).map_or("Druckteil", |p| p.name.as_str()))
                    .show_ui(ui, |ui| {
                        for p in &self.print_parts {
                            ui.selectable_value(&mut self.new_part, p.id, p.name.as_str());
                        }
                    });
                egui::ComboBox::from_id_salt("new_material")
                    .width(180.0)
                    .selected_text(self.materials.iter().find(|m| m.id == self.new_material).map_or("Farbe".to_string(), Material::label))
                    .show_ui(ui, |ui| {
                        for m in &self.materials {
                            ui.selectable_value(&mut self.new_material, m.id, m.label());
                        }
                    });
                let ready = self.new_part != 0 && self.new_material != 0;
                if ui.add_enabled_ui(ready, |ui| widgets::soft_button(ui, "Auswählen")).inner.clicked() {
                    *act = Act::Select(StockKey::PrintPart { print_part_id: self.new_part, material_id: self.new_material });
                }
            });
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
