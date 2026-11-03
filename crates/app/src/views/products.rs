use druckplaner_core::{
    catalog::{BomItem, BomLine, PriceTier, PrintPart, Product, PurchasePart, Surcharge},
    Db,
};
use eframe::egui;

use crate::widgets::{self, Action};

#[derive(Default)]
pub struct Products {
    items: Vec<Product>,
    form: Product,
    print_parts: Vec<PrintPart>,
    purchase_parts: Vec<PurchasePart>,
    kinds: Vec<String>,
}

fn item_label(print: &[PrintPart], purchase: &[PurchasePart], item: BomItem) -> String {
    match item {
        BomItem::PrintPart(id) => print
            .iter()
            .find(|p| p.id == id)
            .map_or_else(|| "(gelöscht)".to_string(), |p| format!("Druckteil: {}", p.name)),
        BomItem::PurchasePart(id) => purchase
            .iter()
            .find(|p| p.id == id)
            .map_or_else(|| "(gelöscht)".to_string(), |p| format!("Zukauf: {}", p.name)),
    }
}

impl Products {
    /// Lädt neben den Produkten auch die Auswahllisten für Stückliste und Aufpreise.
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_products(), "") {
            self.items = v;
        }
        if let Some(v) = widgets::report(status, db.list_print_parts(), "") {
            self.print_parts = v;
        }
        if let Some(v) = widgets::report(status, db.list_purchase_parts(), "") {
            self.purchase_parts = v;
        }
        if let Some(v) = widgets::report(status, db.material_kinds(), "") {
            self.kinds = v;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;

        egui::SidePanel::left("products_list").default_width(240.0).show_inside(ui, |ui| {
            if ui.button("Neues Produkt").clicked() {
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
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(if self.form.id == 0 { "Neues Produkt" } else { "Produkt bearbeiten" });
                ui.add_space(8.0);
                egui::Grid::new("product_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    widgets::text_row(ui, "Name", &mut self.form.name);
                    widgets::int_row(ui, "Arbeitszeit je Stück", &mut self.form.labor_minutes, " min");
                });

                // Stückliste
                ui.add_space(12.0);
                ui.strong("Stückliste (je 1 Stück)");
                if self.print_parts.is_empty() && self.purchase_parts.is_empty() {
                    ui.label("Lege zuerst Druckteile und Zukaufteile an.");
                }
                let mut remove = None;
                for (i, line) in self.form.bom.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("bom_item", i))
                            .width(280.0)
                            .selected_text(item_label(&self.print_parts, &self.purchase_parts, line.item))
                            .show_ui(ui, |ui| {
                                for p in &self.print_parts {
                                    ui.selectable_value(
                                        &mut line.item,
                                        BomItem::PrintPart(p.id),
                                        format!("Druckteil: {}", p.name),
                                    );
                                }
                                for p in &self.purchase_parts {
                                    ui.selectable_value(
                                        &mut line.item,
                                        BomItem::PurchasePart(p.id),
                                        format!("Zukauf: {}", p.name),
                                    );
                                }
                            });
                        ui.add(egui::DragValue::new(&mut line.qty).range(1..=1000).prefix("× "));
                        if ui.small_button("×").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.form.bom.remove(i);
                }
                if ui.button("+ Position").clicked() {
                    let first = self
                        .print_parts
                        .first()
                        .map(|p| BomItem::PrintPart(p.id))
                        .or_else(|| self.purchase_parts.first().map(|p| BomItem::PurchasePart(p.id)));
                    if let Some(item) = first {
                        self.form.bom.push(BomLine { item, qty: 1 });
                    }
                }

                // Staffelpreise
                ui.add_space(12.0);
                ui.strong("Staffelpreise (Stückpreis ab Gesamtmenge je Bestellung)");
                let mut remove = None;
                for (i, t) in self.form.tiers.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label("ab");
                        ui.add(egui::DragValue::new(&mut t.min_qty).range(1..=10_000).suffix(" Stk."));
                        widgets::eur_edit(ui, &mut t.unit_price_cents);
                        if ui.small_button("×").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.form.tiers.remove(i);
                }
                if ui.button("+ Staffel").clicked() {
                    let next = self.form.tiers.iter().map(|t| t.min_qty).max().map_or(1, |m| m + 1);
                    self.form.tiers.push(PriceTier { min_qty: next, unit_price_cents: 0 });
                }

                // Aufpreise je Werkstoff
                ui.add_space(12.0);
                ui.strong("Aufpreis je Werkstoff (pro Stück)");
                let mut remove = None;
                for (i, s) in self.form.surcharges.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("surcharge_kind", i))
                            .width(160.0)
                            .selected_text(s.material_kind.as_str())
                            .show_ui(ui, |ui| {
                                for k in &self.kinds {
                                    ui.selectable_value(&mut s.material_kind, k.clone(), k.as_str());
                                }
                            });
                        widgets::eur_edit(ui, &mut s.cents);
                        if ui.small_button("×").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.form.surcharges.remove(i);
                }
                if ui.button("+ Aufpreis").clicked() {
                    if let Some(k) = self.kinds.first() {
                        self.form.surcharges.push(Surcharge { material_kind: k.clone(), cents: 0 });
                    }
                }

                ui.add_space(12.0);
                widgets::form_buttons(ui, self.form.id != 0, &mut act);
            });
        });

        match act {
            Action::None => {}
            Action::New => self.form = Product::default(),
            Action::Select(id) => {
                if let Some(p) = self.items.iter().find(|p| p.id == id) {
                    self.form = p.clone();
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_product(&mut self.form), "Produkt gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_product(self.form.id), "Produkt gelöscht").is_some() {
                    self.form = Product::default();
                    self.reload(db, status);
                }
            }
        }
    }
}
