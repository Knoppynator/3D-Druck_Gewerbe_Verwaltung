use druckplaner_core::{
    date::format_de,
    invoicing::{Invoice, InvoiceRow, InvoiceState, Payment},
    money::format_eur,
    pdf::letter_texts,
    settings::Sender,
    Db,
};
use eframe::egui;

use crate::widgets;

enum Act {
    None,
    Select(i64),
    OpenPdf,
    Regenerate,
    Cancel,
    AddPayment,
    DeletePayment(i64),
    SetUnits,
}

#[derive(Default)]
pub struct Invoices {
    rows: Vec<InvoiceRow>,
    only_open: bool,
    selected: Option<Invoice>,
    payments: Vec<Payment>,
    new_payment: Payment,
    today: String,
    /// Aktuelle Absenderdaten für die Vorschau
    sender: Sender,
    /// Stückzahl einer Altrechnung (zum Nachtragen)
    units_edit: i64,
}

impl Invoices {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_invoices(), "") {
            self.rows = v;
        }
        if let Some(v) = widgets::report(status, db.today(), "") {
            self.today = v;
        }
        if let Some(v) = widgets::report(status, db.sender(), "") {
            self.sender = v;
        }
        if let Some(id) = self.selected.as_ref().map(|i| i.id) {
            self.select(db, status, id);
        }
    }

    pub fn select(&mut self, db: &Db, status: &mut String, id: i64) {
        let Some(inv) = widgets::report(status, db.load_invoice(id), "") else { return };
        self.payments = widgets::report(status, db.list_payments(id), "").unwrap_or_default();
        let open = self.rows.iter().find(|r| r.id == id).map_or(0, InvoiceRow::open_cents);
        self.new_payment = Payment { invoice_id: id, paid_at: self.today.clone(), amount_cents: open, ..Default::default() };
        self.units_edit = db.invoice_units(id).ok().flatten().unwrap_or(0);
        self.selected = Some(inv);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;

        egui::SidePanel::left("invoices_list").default_width(300.0).show_inside(ui, |ui| {
            ui.checkbox(&mut self.only_open, "Nur offene");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for r in &self.rows {
                    if self.only_open && r.state != InvoiceState::Open {
                        continue;
                    }
                    let overdue = r.state == InvoiceState::Open && r.due_at < self.today;
                    let state = if overdue { "überfällig" } else { r.state.label() };
                    let text = format!(
                        "{} · {}\n{} · {} · {}",
                        r.number,
                        r.customer,
                        format_de(&r.issued_at),
                        format_eur(r.total_cents),
                        state
                    );
                    let selected = self.selected.as_ref().is_some_and(|s| s.id == r.id);
                    if ui.selectable_label(selected, text).clicked() {
                        act = Act::Select(r.id);
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
        let Some(inv) = &self.selected else {
            ui.label("Rechnung links auswählen. Neue Rechnungen entstehen bei den Lieferungen einer Bestellung.");
            return;
        };
        let row = self.rows.iter().find(|r| r.id == inv.id);
        let state = row.map(|r| r.state);

        let storno_of = inv
            .cancels_invoice_id
            .and_then(|id| self.rows.iter().find(|r| r.id == id))
            .map(|r| r.number.as_str());
        let (intro, payment_text) = letter_texts(storno_of, self.sender.payment_days);
        let s = &self.sender;
        let title = if storno_of.is_some() { "Stornorechnung" } else { "Rechnung" };

        ui.heading(format!("{title} {}", inv.number));
        if let Some(r) = row {
            ui.label(format!(
                "Status: {} · bezahlt {} · offen {}",
                r.state.label(),
                format_eur(r.paid_cents),
                format_eur(r.open_cents())
            ));
        }
        ui.add_space(6.0);

        // Vorschau der Beleg-Inhalte (nicht das PDF-Layout)
        egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.columns(2, |cols| {
                for line in inv.recipient.lines() {
                    cols[0].label(line);
                }
                cols[1].with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                    for line in [s.name.as_str(), s.street.as_str(), s.zip_city.as_str()] {
                        if !line.is_empty() {
                            ui.label(line);
                        }
                    }
                    if !s.tax_id.is_empty() {
                        ui.label(format!("Steuer-ID: {}", s.tax_id));
                    }
                    if !s.income_tax_no.is_empty() {
                        ui.label(format!("Einkommensteuernr.: {}", s.income_tax_no));
                    }
                });
            });
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                ui.label(format!("Datum: {}", format_de(&inv.issued_at)));
                ui.label(format!("Rechnungsnummer: {}", inv.number));
                if storno_of.is_none() {
                    ui.weak(format!("fällig bis {}", format_de(&inv.due_at)));
                }
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(title).size(16.0).strong());
            if let Some(n) = storno_of {
                ui.label(format!("Storno zu Rechnung {n}"));
            }
            ui.add_space(6.0);
            ui.label(inv.salutation.as_str());
            ui.label(intro);
            ui.add_space(6.0);

            egui::Grid::new("invoice_lines").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                for h in ["Pos.", "Anzahl", "Einheit", "Bezeichnung", "Einzelpreis", "Gesamtpreis"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, l) in inv.lines.iter().enumerate() {
                    ui.label((i + 1).to_string());
                    ui.label(l.qty.to_string());
                    ui.label(l.unit.as_str());
                    ui.label(l.description.replace('\n', " · "));
                    ui.label(format_eur(l.unit_price_cents));
                    ui.label(format_eur(l.total_cents()));
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                ui.label(egui::RichText::new(format!("Rechnungsbetrag: {}", format_eur(inv.total_cents))).strong());
            });
            if !payment_text.is_empty() {
                ui.add_space(6.0);
                ui.label(payment_text);
            }
            if !inv.footer.is_empty() {
                ui.add_space(4.0);
                ui.weak(inv.footer.as_str());
            }
            if !s.iban.is_empty() {
                ui.add_space(6.0);
                ui.weak(format!("{} · IBAN {} · BIC {} · {}", s.bank_name, s.iban, s.bic, s.account_holder));
            }
        });

        if inv.lines.iter().any(|l| l.kind == "legacy") {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Stückzahl dieser Altrechnung (fürs Dashboard)");
                ui.add(egui::DragValue::new(&mut self.units_edit).range(0..=100_000).suffix(" Stk."));
                if ui.button("Speichern").clicked() {
                    *act = Act::SetUnits;
                }
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("PDF öffnen").clicked() {
                *act = Act::OpenPdf;
            }
            if ui.button("PDF neu erzeugen").on_hover_text("Nutzt die aktuelle Vorlage und die aktuellen Absenderdaten").clicked() {
                *act = Act::Regenerate;
            }
            if state == Some(InvoiceState::Open) && self.payments.is_empty() && ui.button("Stornieren").clicked() {
                *act = Act::Cancel;
            }
        });

        if matches!(state, Some(InvoiceState::Open) | Some(InvoiceState::Paid)) {
            ui.add_space(16.0);
            ui.separator();
            ui.heading("Zahlungen");
            for p in &self.payments {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {}", format_de(&p.paid_at), format_eur(p.amount_cents)));
                    if !p.note.is_empty() {
                        ui.weak(p.note.as_str());
                    }
                    if ui.small_button("×").on_hover_text("Zahlung entfernen").clicked() {
                        *act = Act::DeletePayment(p.id);
                    }
                });
            }
            ui.horizontal(|ui| {
                widgets::date_edit(ui, "pay_date", &mut self.new_payment.paid_at);
                widgets::eur_edit(ui, &mut self.new_payment.amount_cents);
                ui.add(egui::TextEdit::singleline(&mut self.new_payment.note).desired_width(160.0).hint_text("Notiz"));
                if ui.button("Zahlung erfassen").clicked() {
                    *act = Act::AddPayment;
                }
            });
        }
    }

    fn handle(&mut self, act: Act, db: &Db, status: &mut String) {
        let id = self.selected.as_ref().map_or(0, |i| i.id);
        match act {
            Act::None => {}
            Act::Select(id) => self.select(db, status, id),
            Act::OpenPdf => {
                if let Some(inv) = &self.selected {
                    widgets::open_file(&db.invoice_pdf_file(inv), status);
                }
            }
            Act::Regenerate => {
                if let Some(path) = widgets::report(status, db.regenerate_invoice_pdf(id), "PDF neu erzeugt") {
                    widgets::open_file(&path, status);
                }
            }
            Act::Cancel => {
                if let Some(storno) = widgets::report(status, db.cancel_invoice(id), "") {
                    *status = "Rechnung storniert, Stornobeleg erstellt".to_string();
                    self.reload(db, status);
                    self.select(db, status, storno);
                }
            }
            Act::SetUnits => {
                let units = (self.units_edit > 0).then_some(self.units_edit);
                widgets::report(status, db.set_invoice_units(id, units), "Stückzahl gespeichert");
            }
            Act::AddPayment => {
                if widgets::report(status, db.add_payment(&mut self.new_payment), "Zahlung erfasst").is_some() {
                    self.reload(db, status);
                }
            }
            Act::DeletePayment(pid) => {
                if widgets::report(status, db.delete_payment(pid), "Zahlung entfernt").is_some() {
                    self.reload(db, status);
                }
            }
        }
    }
}
