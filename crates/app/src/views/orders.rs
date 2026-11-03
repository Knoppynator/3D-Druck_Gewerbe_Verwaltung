use std::collections::HashMap;

use druckplaner_core::{
    catalog::{Customer, Material, Product, PurchasePart},
    date::format_de,
    money::format_eur,
    orders::{apply_prices, suggest_due_date, Order, OrderLine, OrderStatus, OrderSummary, STAGES},
    shipments::Shipment,
    Db,
};
use eframe::egui;

use crate::widgets;

/// Lieferung im Bearbeitungszustand: `orig` ist der gespeicherte Stand, `max` die je Position
/// noch verteilbare Menge (ohne diese Lieferung).
struct ShipEdit {
    sh: Shipment,
    orig: Shipment,
    max: HashMap<i64, i64>,
}

enum Act {
    None,
    New,
    Select(i64),
    Save,
    Delete,
    NewShipment,
    SaveShipment(usize),
    DeleteShipment(usize),
    CreateInvoice(usize),
    OpenInvoicePdf(i64),
}

#[derive(Default)]
pub struct Orders {
    summaries: Vec<OrderSummary>,
    show_closed: bool,
    form: Order,
    ships: Vec<ShipEdit>,
    customers: Vec<Customer>,
    products: Vec<Product>,
    materials: Vec<Material>,
    parts: Vec<PurchasePart>,
    today: String,
    /// Standard-Lieferzeit in Tagen (Einstellungen) für den Terminvorschlag
    default_lead_days: i64,
    /// Wunschtermin folgt dem Vorschlag, solange er nicht von Hand geändert wurde
    due_auto: bool,
}

impl Orders {
    /// Lädt Listen und Auswahldaten neu; die geöffnete Bestellung bleibt (samt ungespeicherten Änderungen).
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_order_summaries(), "") {
            self.summaries = v;
        }
        if let Some(v) = widgets::report(status, db.list_customers(), "") {
            self.customers = v;
        }
        if let Some(v) = widgets::report(status, db.list_products(), "") {
            self.products = v;
        }
        if let Some(v) = widgets::report(status, db.list_materials(), "") {
            self.materials = v;
        }
        if let Some(v) = widgets::report(status, db.list_purchase_parts(), "") {
            self.parts = v;
        }
        if let Some(v) = widgets::report(status, db.today(), "") {
            self.today = v;
        }
        if let Some(v) = widgets::report(status, db.setting_or_default("default_lead_days"), "") {
            self.default_lead_days = v.trim().parse().unwrap_or(0);
        }
        self.reload_ships(db, status);
    }

    fn reload_ships(&mut self, db: &Db, status: &mut String) {
        self.ships.clear();
        if self.form.id == 0 {
            return;
        }
        let Some(list) = widgets::report(status, db.list_shipments(self.form.id), "") else { return };
        for sh in list {
            let max = widgets::report(status, db.remaining_by_line(self.form.id, sh.id), "")
                .map(|v| v.into_iter().collect())
                .unwrap_or_default();
            self.ships.push(ShipEdit { orig: sh.clone(), sh, max });
        }
    }

    /// Öffnet eine Bestellung (auch aus anderen Seiten).
    pub fn open(&mut self, db: &Db, status: &mut String, id: i64) {
        if let Some(order) = widgets::report(status, db.load_order(id), "") {
            self.form = order;
            self.due_auto = self.form.due_date.is_empty();
            self.reload_ships(db, status);
        }
    }

    fn new_order(&mut self) {
        self.form = Order {
            customer_id: self.customers.first().map_or(0, |c| c.id),
            received_at: self.today.clone(),
            ..Default::default()
        };
        self.due_auto = true;
        self.ships.clear();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;

        egui::SidePanel::left("orders_list").default_width(300.0).show_inside(ui, |ui| {
            if ui.button("Neue Bestellung").clicked() {
                act = Act::New;
            }
            ui.checkbox(&mut self.show_closed, "Abgeschlossene zeigen");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in &self.summaries {
                    let st = s.status();
                    if st == OrderStatus::Paid && !self.show_closed && self.form.id != s.id {
                        continue;
                    }
                    let text = format!(
                        "{}\n{} · {} · {}",
                        s.customer,
                        format_de(&s.received_at),
                        st.label(),
                        format_eur(s.total_cents)
                    );
                    if ui.selectable_label(self.form.id == s.id, text).clicked() {
                        act = Act::Select(s.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.detail_ui(ui, &mut act));
        });

        self.handle(act, db, status);
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let (received_before, due_before) = (self.form.received_at.clone(), self.form.due_date.clone());
        let mut suggest_now = false;
        ui.heading(if self.form.id == 0 { "Neue Bestellung" } else { "Bestellung bearbeiten" });
        ui.add_space(8.0);

        egui::Grid::new("order_head").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Kunde");
            egui::ComboBox::from_id_salt("order_customer")
                .width(260.0)
                .selected_text(
                    self.customers
                        .iter()
                        .find(|c| c.id == self.form.customer_id)
                        .map_or("– wählen –", |c| c.company.as_str()),
                )
                .show_ui(ui, |ui| {
                    for c in &self.customers {
                        ui.selectable_value(&mut self.form.customer_id, c.id, c.company.as_str());
                    }
                });
            ui.end_row();
            widgets::date_row(ui, "Eingang", &mut self.form.received_at);
            ui.label("Wunschtermin");
            ui.horizontal(|ui| {
                widgets::date_edit(ui, "Wunschtermin", &mut self.form.due_date);
                if ui
                    .button("Vorschlag")
                    .on_hover_text("Eingang plus Standard-Lieferzeit je nach Stückzahl (Produkt-Staffel bzw. Einstellungen)")
                    .clicked()
                {
                    suggest_now = true;
                }
            });
            ui.end_row();
            widgets::text_row(ui, "Kanal (z. B. WhatsApp)", &mut self.form.channel);
            widgets::text_row(ui, "Notiz", &mut self.form.note);
            if self.ships.is_empty() {
                ui.label("Stand");
                egui::ComboBox::from_id_salt("order_stage")
                    .selected_text(STAGES[self.form.stage.clamp(0, 2) as usize])
                    .show_ui(ui, |ui| {
                        for (i, name) in STAGES.iter().enumerate() {
                            ui.selectable_value(&mut self.form.stage, i as i64, *name);
                        }
                    });
                ui.end_row();
            }
        });
        if self.form.due_date != due_before {
            self.due_auto = false;
        }
        if suggest_now {
            self.due_auto = true;
        }

        // Positionen
        ui.add_space(12.0);
        ui.strong("Positionen");
        let mut remove = None;
        let mut reprice = false;
        let delivered: Vec<i64> = self.ships.iter().flat_map(|e| e.orig.lines.iter().map(|l| l.order_line_id)).collect();
        for (i, l) in self.form.lines.iter_mut().enumerate() {
            let before = (l.product_id, l.material_id, l.purchase_part_id, l.qty);
            ui.horizontal(|ui| {
                if l.product_id.is_some() {
                    let name = self
                        .products
                        .iter()
                        .find(|p| Some(p.id) == l.product_id)
                        .map_or("(gelöscht)", |p| p.name.as_str());
                    egui::ComboBox::from_id_salt(("line_product", i)).width(170.0).selected_text(name).show_ui(
                        ui,
                        |ui| {
                            for p in &self.products {
                                ui.selectable_value(&mut l.product_id, Some(p.id), p.name.as_str());
                            }
                        },
                    );
                    let material = self
                        .materials
                        .iter()
                        .find(|m| Some(m.id) == l.material_id)
                        .map_or("(kein Material)".to_string(), |m| m.label());
                    egui::ComboBox::from_id_salt(("line_material", i)).width(150.0).selected_text(material).show_ui(
                        ui,
                        |ui| {
                            ui.selectable_value(&mut l.material_id, None, "(kein Material)");
                            for m in &self.materials {
                                ui.selectable_value(&mut l.material_id, Some(m.id), m.label());
                            }
                        },
                    );
                } else if l.purchase_part_id.is_some() {
                    let name = self
                        .parts
                        .iter()
                        .find(|p| Some(p.id) == l.purchase_part_id)
                        .map_or("(gelöscht)", |p| p.name.as_str());
                    let current = l.purchase_part_id;
                    egui::ComboBox::from_id_salt(("line_part", i)).width(324.0).selected_text(name).show_ui(
                        ui,
                        |ui| {
                            for p in self.parts.iter().filter(|p| p.sellable || Some(p.id) == current) {
                                ui.selectable_value(&mut l.purchase_part_id, Some(p.id), p.name.as_str());
                            }
                        },
                    );
                } else {
                    ui.add(egui::TextEdit::singleline(&mut l.description).desired_width(324.0).hint_text("Bezeichnung"));
                }
                ui.add(egui::DragValue::new(&mut l.qty).range(1..=100_000).suffix(" Stk."));
                widgets::eur_edit(ui, &mut l.unit_price_cents);
                ui.label(format_eur(l.total_cents()));
                let locked = l.id != 0 && delivered.contains(&l.id);
                if ui.add_enabled(!locked, egui::Button::new("×").small()).on_disabled_hover_text("Bereits geliefert").clicked() {
                    remove = Some(i);
                }
            });
            if before != (l.product_id, l.material_id, l.purchase_part_id, l.qty) {
                reprice = true;
            }
        }
        if let Some(i) = remove {
            self.form.lines.remove(i);
            reprice = true;
        }
        ui.horizontal(|ui| {
            if ui.button("+ Produkt").clicked() {
                if let Some(p) = self.products.first() {
                    self.form.lines.push(OrderLine {
                        product_id: Some(p.id),
                        material_id: self.materials.first().map(|m| m.id),
                        ..Default::default()
                    });
                    reprice = true;
                }
            }
            if ui.button("+ Zukaufteil").clicked() {
                if let Some(p) = self.parts.iter().find(|p| p.sellable) {
                    self.form.lines.push(OrderLine { purchase_part_id: Some(p.id), unit: p.unit.clone(), ..Default::default() });
                    reprice = true;
                }
            }
            if ui.button("+ Freie Position").clicked() {
                self.form.lines.push(OrderLine::default());
            }
            if ui.button("Preise aus Preisliste").on_hover_text("Setzt alle Stückpreise neu (Staffel nach Gesamtmenge je Produkt)").clicked() {
                reprice = true;
            }
        });
        if reprice {
            apply_prices(&mut self.form, &self.products, &self.materials, &self.parts);
        }
        if (reprice || suggest_now || self.form.received_at != received_before) && self.due_auto {
            if let Some(due) = suggest_due_date(&self.form, &self.products, self.default_lead_days) {
                self.form.due_date = due;
            }
        }
        ui.label(format!("Summe: {}", format_eur(self.form.total_cents())));

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Speichern").clicked() {
                *act = Act::Save;
            }
            if self.form.id != 0 && self.ships.is_empty() && ui.button("Löschen").clicked() {
                *act = Act::Delete;
            }
        });

        if self.form.id != 0 {
            self.shipments_ui(ui, act);
        }
    }

    fn shipments_ui(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        ui.add_space(16.0);
        ui.separator();
        ui.heading("Lieferungen");
        for (idx, e) in self.ships.iter_mut().enumerate() {
            let invoice_info = e.sh.invoice_number.as_deref().map_or(String::new(), |n| format!(" · Rechnung {n}"));
            let state = if e.sh.is_shipped() {
                format!("versendet {}", format_de(&e.sh.shipped_at))
            } else {
                "verpackt".to_string()
            };
            let qty: i64 = e.sh.lines.iter().map(|l| l.qty).sum();
            let title = format!("Lieferung {} · {qty} Stk. · {state}{invoice_info}", idx + 1);
            egui::CollapsingHeader::new(title).id_salt(("ship", idx)).default_open(true).show(ui, |ui| {
                let locked = e.sh.invoice_number.is_some();
                ui.add_enabled_ui(!locked, |ui| {
                    egui::Grid::new(("ship_grid", idx)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        ui.label("Gepackt");
                        widgets::date_edit(ui, ("packed", idx), &mut e.sh.packed_at);
                        ui.end_row();
                        ui.label("Versendet");
                        ui.horizontal(|ui| {
                            widgets::date_edit(ui, ("shipped", idx), &mut e.sh.shipped_at);
                            if ui.button("heute").clicked() {
                                e.sh.shipped_at = self.today.clone();
                            }
                        });
                        ui.end_row();
                        widgets::eur_row(ui, "Versandkosten (Rechnung)", &mut e.sh.shipping_charge_cents);
                        widgets::text_row(ui, "Sendungsnummer", &mut e.sh.tracking);
                        widgets::text_row(ui, "Notiz", &mut e.sh.note);
                    });
                    ui.add_space(4.0);
                    for ol in self.form.lines.iter().filter(|l| l.id != 0) {
                        let max = e.max.get(&ol.id).copied().unwrap_or(0);
                        let mut q = e.sh.qty_of(ol.id);
                        if max == 0 && q == 0 {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            if ui.add(egui::DragValue::new(&mut q).range(0..=max).suffix(" Stk.")).changed() {
                                e.sh.set_qty(ol.id, q);
                            }
                            ui.label(format!("von {max} · {}", ol.description.replace('\n', " ")));
                        });
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if !locked && ui.button("Speichern").clicked() {
                        *act = Act::SaveShipment(idx);
                    }
                    if !locked && ui.button("Löschen").clicked() {
                        *act = Act::DeleteShipment(idx);
                    }
                    match (e.sh.invoice_id, e.sh.id != 0 && e.sh.is_shipped()) {
                        (Some(inv), _) => {
                            if ui.button("Rechnung (PDF) öffnen").clicked() {
                                *act = Act::OpenInvoicePdf(inv);
                            }
                        }
                        (None, true) if e.sh == e.orig => {
                            if ui.button("Rechnung erstellen").clicked() {
                                *act = Act::CreateInvoice(idx);
                            }
                        }
                        (None, true) => {
                            ui.weak("Rechnung erst nach dem Speichern möglich");
                        }
                        (None, false) => {}
                    }
                });
            });
        }
        if ui.button("+ Lieferung (Restmengen)").clicked() {
            *act = Act::NewShipment;
        }
    }

    fn handle(&mut self, act: Act, db: &Db, status: &mut String) {
        match act {
            Act::None => {}
            Act::New => self.new_order(),
            Act::Select(id) => self.open(db, status, id),
            Act::Save => {
                if widgets::report(status, db.save_order(&mut self.form), "Bestellung gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Act::Delete => {
                if widgets::report(status, db.delete_order(self.form.id), "Bestellung gelöscht").is_some() {
                    self.new_order();
                    self.reload(db, status);
                }
            }
            Act::NewShipment => {
                let Some(draft) = widgets::report(status, db.new_shipment_draft(self.form.id), "") else { return };
                if draft.lines.is_empty() {
                    *status = "Alle Mengen sind bereits auf Lieferungen verteilt".to_string();
                    return;
                }
                let max = widgets::report(status, db.remaining_by_line(self.form.id, 0), "")
                    .map(|v| v.into_iter().collect())
                    .unwrap_or_default();
                self.ships.push(ShipEdit { sh: draft, orig: Shipment::default(), max });
            }
            Act::SaveShipment(i) => {
                if widgets::report(status, db.save_shipment(&mut self.ships[i].sh), "Lieferung gespeichert").is_some() {
                    self.reload(db, status);
                }
            }
            Act::DeleteShipment(i) => {
                if self.ships[i].sh.id == 0 {
                    self.ships.remove(i);
                } else if widgets::report(status, db.delete_shipment(self.ships[i].sh.id), "Lieferung gelöscht").is_some() {
                    self.reload(db, status);
                }
            }
            Act::CreateInvoice(i) => {
                let result = db.create_invoice(self.ships[i].sh.id).and_then(|id| db.load_invoice(id));
                if let Some(inv) = widgets::report(status, result, "") {
                    *status = format!("Rechnung {} erstellt", inv.number);
                    widgets::open_file(&db.invoice_pdf_file(&inv), status);
                    self.reload(db, status);
                }
            }
            Act::OpenInvoicePdf(id) => {
                if let Some(inv) = widgets::report(status, db.load_invoice(id), "") {
                    widgets::open_file(&db.invoice_pdf_file(&inv), status);
                }
            }
        }
    }
}
