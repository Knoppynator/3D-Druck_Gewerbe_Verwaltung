use druckplaner_core::{
    catalog::{Material, PurchasePart},
    date::format_de,
    finance::{Category, Expense, ExpenseRow},
    money::format_eur,
    stock::StockKey,
    Db,
};
use eframe::egui;

use crate::widgets;

enum Act {
    None,
    New,
    Select(i64),
    Save,
    Delete,
    AttachFile,
    OpenReceipt,
    ClearReceipt,
    Year(Option<i32>),
    BookReceipt,
    DeleteReceipt(i64),
}

#[derive(Default)]
pub struct Expenses {
    rows: Vec<ExpenseRow>,
    years: Vec<i32>,
    year: Option<i32>,
    only_missing: bool,
    form: Expense,
    categories: Vec<Category>,
    shipments: Vec<(i64, String)>,
    today: String,
    // Wareneingang (Lagerbuchung) zur gewählten Ausgabe
    materials: Vec<Material>,
    parts: Vec<PurchasePart>,
    /// bereits gebuchte Eingänge dieser Ausgabe: (Bewegungs-Id, Text)
    receipts: Vec<(i64, String)>,
    rc_filament: bool,
    rc_item: i64,
    rc_qty: i64,
    rc_update_price: bool,
}

impl Expenses {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_expenses(self.year), "") {
            self.rows = v;
        }
        if let Some(v) = widgets::report(status, db.finance_years(), "") {
            self.years = v;
        }
        if let Some(v) = widgets::report(status, db.list_categories(), "") {
            self.categories = v;
        }
        if let Some(v) = widgets::report(status, db.shipment_choices(), "") {
            self.shipments = v;
        }
        if let Some(v) = widgets::report(status, db.today(), "") {
            self.today = v;
        }
        if let Some(v) = widgets::report(status, db.list_materials(), "") {
            self.materials = v;
        }
        if let Some(v) = widgets::report(status, db.list_purchase_parts(), "") {
            self.parts = v;
        }
        // gespeicherte Ausgabe frisch laden (Beleg-Verknüpfung kann sich geändert haben)
        if self.form.id != 0 {
            if let Some(e) = widgets::report(status, db.load_expense(self.form.id), "") {
                self.form = e;
            }
        }
        self.load_receipts(db, status);
    }

    /// Lädt die zu dieser Ausgabe gebuchten Wareneingänge.
    fn load_receipts(&mut self, db: &Db, status: &mut String) {
        self.receipts.clear();
        if self.form.id == 0 {
            return;
        }
        let Some(moves) = widgets::report(status, db.stock_moves_for_ref("expense", self.form.id), "") else { return };
        for m in moves {
            let (name, unit) = db.stock_label(m.key).unwrap_or_default();
            self.receipts.push((m.id, format!("{name}: +{} {unit}", m.qty)));
        }
    }

    fn new_expense(&mut self) {
        self.receipts.clear();
        self.form = Expense {
            spent_at: self.today.clone(),
            category_id: self.categories.first().map_or(0, |c| c.id),
            ..Default::default()
        };
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;

        egui::SidePanel::left("expenses_list").default_width(320.0).show_inside(ui, |ui| {
            if ui.button("Neue Ausgabe").clicked() {
                act = Act::New;
            }
            ui.horizontal(|ui| {
                ui.label("Jahr");
                egui::ComboBox::from_id_salt("expense_year")
                    .selected_text(self.year.map_or("Alle".to_string(), |y| y.to_string()))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.year.is_none(), "Alle").clicked() {
                            act = Act::Year(None);
                        }
                        for y in &self.years {
                            if ui.selectable_label(self.year == Some(*y), y.to_string()).clicked() {
                                act = Act::Year(Some(*y));
                            }
                        }
                    });
            });
            ui.checkbox(&mut self.only_missing, "Nur ohne Beleg");
            let sum: i64 = self.rows.iter().map(|r| r.expense.amount_cents).sum();
            ui.weak(format!("{} Ausgaben · {}", self.rows.len(), format_eur(sum)));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for r in &self.rows {
                    let e = &r.expense;
                    if self.only_missing && e.has_receipt() {
                        continue;
                    }
                    let missing = if e.has_receipt() { "" } else { " · Beleg fehlt" };
                    let text = format!(
                        "{} · {}\n{} ({}){}",
                        format_de(&e.spent_at),
                        format_eur(e.amount_cents),
                        e.description,
                        r.category,
                        missing
                    );
                    if ui.selectable_label(self.form.id == e.id, text).clicked() {
                        act = Act::Select(e.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.detail_ui(ui, db, &mut act));
        });

        self.handle(act, db, status);
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, db: &Db, act: &mut Act) {
        ui.heading(if self.form.id == 0 { "Neue Ausgabe" } else { "Ausgabe bearbeiten" });
        ui.add_space(8.0);
        egui::Grid::new("expense_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            widgets::date_row(ui, "Datum", &mut self.form.spent_at);
            widgets::text_row(ui, "Bezeichnung", &mut self.form.description);
            ui.label("Menge");
            ui.add(egui::DragValue::new(&mut self.form.qty).range(0..=1_000_000));
            ui.end_row();
            widgets::text_row(ui, "Lieferant", &mut self.form.vendor);
            ui.label("Kategorie");
            egui::ComboBox::from_id_salt("expense_category")
                .width(240.0)
                .selected_text(
                    self.categories
                        .iter()
                        .find(|c| c.id == self.form.category_id)
                        .map_or("– wählen –", |c| c.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for c in &self.categories {
                        ui.selectable_value(&mut self.form.category_id, c.id, c.name.as_str());
                    }
                });
            ui.end_row();
            widgets::eur_row(ui, "Betrag", &mut self.form.amount_cents);
            ui.label("Zu Lieferung (Porto)");
            egui::ComboBox::from_id_salt("expense_shipment")
                .width(320.0)
                .selected_text(
                    self.shipments
                        .iter()
                        .find(|(id, _)| Some(*id) == self.form.shipment_id)
                        .map_or("(keine)", |(_, label)| label.as_str()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form.shipment_id, None, "(keine)");
                    for (id, label) in &self.shipments {
                        ui.selectable_value(&mut self.form.shipment_id, Some(*id), label.as_str());
                    }
                });
            ui.end_row();
            widgets::text_row(ui, "Notiz", &mut self.form.note);
        });

        ui.add_space(10.0);
        ui.strong("Beleg");
        ui.checkbox(&mut self.form.receipt_present, "Beleg liegt vor");
        if self.form.id == 0 {
            ui.weak("Eine Belegdatei lässt sich nach dem ersten Speichern anhängen.");
        } else {
            ui.horizontal(|ui| {
                if ui.button("Datei anhängen …").clicked() {
                    *act = Act::AttachFile;
                }
                if let Some(file) = db.receipt_file(&self.form) {
                    if ui.button("Beleg öffnen").clicked() {
                        *act = Act::OpenReceipt;
                    }
                    if ui.button("Verknüpfung lösen").clicked() {
                        *act = Act::ClearReceipt;
                    }
                    ui.weak(file.file_name().and_then(|n| n.to_str()).unwrap_or(""));
                }
            });
        }

        if self.form.id != 0 {
            ui.add_space(10.0);
            ui.strong("Wareneingang (Lager)");
            for (id, text) in &self.receipts {
                ui.horizontal(|ui| {
                    ui.label(text.as_str());
                    if ui.small_button("×").on_hover_text("Eingang löschen").clicked() {
                        *act = Act::DeleteReceipt(*id);
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.rc_filament, true, "Filament");
                ui.selectable_value(&mut self.rc_filament, false, "Zukaufteil");
                let selected_text = if self.rc_filament {
                    self.materials.iter().find(|m| m.id == self.rc_item).map_or("wählen".to_string(), Material::label)
                } else {
                    self.parts.iter().find(|p| p.id == self.rc_item).map_or("wählen".to_string(), |p| p.name.clone())
                };
                egui::ComboBox::from_id_salt("receipt_item").width(200.0).selected_text(selected_text).show_ui(ui, |ui| {
                    if self.rc_filament {
                        for m in &self.materials {
                            ui.selectable_value(&mut self.rc_item, m.id, m.label());
                        }
                    } else {
                        for p in &self.parts {
                            ui.selectable_value(&mut self.rc_item, p.id, p.name.as_str());
                        }
                    }
                });
                let unit = if self.rc_filament { " g" } else { " Stk" };
                ui.add(egui::DragValue::new(&mut self.rc_qty).range(0..=10_000_000).suffix(unit));
                ui.checkbox(&mut self.rc_update_price, "Einkaufspreis aktualisieren");
                if ui.add_enabled(self.rc_item != 0 && self.rc_qty > 0, egui::Button::new("Wareneingang buchen")).clicked() {
                    *act = Act::BookReceipt;
                }
            });
            ui.weak("Der Preis wird aus dem gesamten Betrag dieser Ausgabe und der gebuchten Menge berechnet.");
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Speichern").clicked() {
                *act = Act::Save;
            }
            if self.form.id != 0 && ui.button("Löschen").clicked() {
                *act = Act::Delete;
            }
        });
    }

    fn handle(&mut self, act: Act, db: &Db, status: &mut String) {
        match act {
            Act::None => {}
            Act::New => self.new_expense(),
            Act::Select(id) => {
                if let Some(e) = widgets::report(status, db.load_expense(id), "") {
                    self.form = e;
                    self.load_receipts(db, status);
                }
            }
            Act::Year(year) => {
                self.year = year;
                self.reload(db, status);
            }
            Act::Save => {
                if widgets::report(status, db.save_expense(&mut self.form), "Ausgabe gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Act::Delete => {
                if widgets::report(status, db.delete_expense(self.form.id), "Ausgabe gelöscht (Belegdatei bleibt im Ordner)").is_some() {
                    self.new_expense();
                    self.reload(db, status);
                }
            }
            Act::AttachFile => {
                let picked = rfd::FileDialog::new()
                    .set_title("Beleg auswählen")
                    .add_filter("Belege", &["pdf", "png", "jpg", "jpeg"])
                    .add_filter("Alle Dateien", &["*"])
                    .pick_file();
                if let Some(path) = picked {
                    // ungespeicherte Änderungen zuerst sichern, damit Datum/Jahr für den Ablageort stimmen
                    if widgets::report(status, db.save_expense(&mut self.form), "").is_none() {
                        return;
                    }
                    if widgets::report(status, db.attach_receipt(self.form.id, &path), "Beleg abgelegt").is_some() {
                        self.reload(db, status);
                    }
                }
            }
            Act::OpenReceipt => {
                if let Some(path) = db.receipt_file(&self.form) {
                    widgets::open_file(&path, status);
                }
            }
            Act::ClearReceipt => {
                if widgets::report(status, db.clear_receipt(self.form.id), "Verknüpfung gelöst (Datei bleibt im Ordner)").is_some() {
                    self.reload(db, status);
                }
            }
            Act::BookReceipt => {
                let key = if self.rc_filament {
                    StockKey::Filament { material_id: self.rc_item }
                } else {
                    StockKey::PurchasePart { purchase_part_id: self.rc_item }
                };
                if widgets::report(status, db.book_receipt(self.form.id, key, self.rc_qty, self.rc_update_price), "Wareneingang gebucht").is_some() {
                    self.rc_qty = 0;
                    self.reload(db, status);
                }
            }
            Act::DeleteReceipt(id) => {
                if widgets::report(status, db.delete_stock_move(id), "Wareneingang gelöscht").is_some() {
                    self.reload(db, status);
                }
            }
        }
    }
}
