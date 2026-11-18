use druckplaner_core::{
    date::format_de,
    invoicing::{Invoice, InvoiceRow, InvoiceState, Payment},
    money::format_eur,
    pdf::letter_texts,
    settings::Sender,
    Db,
};
use eframe::egui;

use crate::{
    icons, theme,
    widgets::{self, Tone},
};

/// Status-Chip einer Rechnung; offene nach Fälligkeit als überfällig.
fn state_chip(r: &InvoiceRow, today: &str) -> (Tone, &'static str) {
    match r.state {
        InvoiceState::Open if r.due_at.as_str() < today => (Tone::Critical, "überfällig"),
        InvoiceState::Open => (Tone::Warn, "offen"),
        InvoiceState::Paid => (Tone::Good, "bezahlt"),
        InvoiceState::Cancelled => (Tone::Neutral, "storniert"),
        InvoiceState::Storno => (Tone::Neutral, "Stornobeleg"),
    }
}

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

        let open: Vec<&InvoiceRow> = self.rows.iter().filter(|r| r.state == InvoiceState::Open).collect();
        let overdue = open.iter().filter(|r| r.due_at < self.today).count();
        let owed: i64 = open.iter().map(|r| r.open_cents()).sum();
        let subtitle = format!("{} offen ({}) · {overdue} überfällig · neue Rechnungen entstehen bei den Lieferungen", open.len(), format_eur(owed));
        widgets::page_header(ui, "Rechnungen", &subtitle, |_| {});

        widgets::list_panel(ui, "invoices_list", |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.only_open, "Offen").clicked() {
                    self.only_open = true;
                }
                if ui.selectable_label(!self.only_open, "Alle").clicked() {
                    self.only_open = false;
                }
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                for r in &self.rows {
                    if self.only_open && r.state != InvoiceState::Open {
                        continue;
                    }
                    let title = format!("{} · {}", r.number, r.customer);
                    let total = format_eur(r.total_cents);
                    let meta = format!("vom {} · fällig {}", format_de(&r.issued_at), format_de(&r.due_at));
                    let mut entry = widgets::ListEntry::new(&title);
                    entry.right = &total;
                    entry.meta = &meta;
                    entry.chip = Some(state_chip(r, &self.today));
                    let selected = self.selected.as_ref().is_some_and(|s| s.id == r.id);
                    if widgets::list_entry(ui, ("invoice", r.id), selected, &entry).clicked() {
                        act = Act::Select(r.id);
                    }
                }
            });
        });

        widgets::detail_panel(ui, |ui| self.detail_ui(ui, &mut act));

        self.handle(act, db, status);
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let Some(inv) = &self.selected else {
            widgets::card(ui, "Keine Rechnung gewählt", "", |ui| {
                widgets::hint(ui, "Rechnung links auswählen. Neue Rechnungen entstehen bei den Lieferungen einer Bestellung.");
            });
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

        // Kopfkarte mit Status und Aktionen
        widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    widgets::hint(ui, &format!("{} · vom {}", inv.recipient.lines().next().unwrap_or(""), format_de(&inv.issued_at)));
                    ui.horizontal(|ui| {
                        ui.label(theme::strong(format!("{title} {}", inv.number), 20.0));
                        if let Some(r) = row {
                            let (tone, label) = state_chip(r, &self.today);
                            widgets::chip(ui, tone, label);
                        }
                    });
                    if let Some(r) = row {
                        ui.label(
                            egui::RichText::new(format!("Betrag {} · bezahlt {} · offen {}", format_eur(r.total_cents), format_eur(r.paid_cents), format_eur(r.open_cents())))
                                .color(theme::p().text_soft),
                        );
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "PDF öffnen").clicked() {
                        *act = Act::OpenPdf;
                    }
                    if ui.button("PDF neu erzeugen").on_hover_text("Nutzt die aktuelle Vorlage und die aktuellen Absenderdaten").clicked() {
                        *act = Act::Regenerate;
                    }
                    if state == Some(InvoiceState::Open) && self.payments.is_empty() && widgets::danger_button(ui, "Stornieren").clicked() {
                        *act = Act::Cancel;
                    }
                });
            });
        });
        ui.add_space(4.0);

        // Vorschau der Beleg-Inhalte (nicht das PDF-Layout)
        widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::card_title(ui, "Vorschau", "Inhalt der Rechnung, nicht das PDF-Layout", |_| {});
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

        ui.add_space(4.0);
        if inv.lines.iter().any(|l| l.kind == "legacy") {
            widgets::card(ui, "Altrechnung", "Stückzahl fürs Dashboard nachtragen", |ui| {
                ui.horizontal(|ui| {
                    widgets::form_label(ui, "Stückzahl");
                    ui.add(egui::DragValue::new(&mut self.units_edit).range(0..=100_000).suffix(" Stk."));
                    if widgets::soft_button(ui, "Speichern").clicked() {
                        *act = Act::SetUnits;
                    }
                });
            });
        }

        if matches!(state, Some(InvoiceState::Open) | Some(InvoiceState::Paid)) {
            widgets::card(ui, "Zahlungen", "", |ui| {
                if self.payments.is_empty() {
                    widgets::hint(ui, "Noch keine Zahlung erfasst.");
                }
                for p in &self.payments {
                    ui.horizontal(|ui| {
                        icons::show(ui, icons::Icon::Check, 14.0, theme::p().good_text);
                        ui.label(format!("{} · {}", format_de(&p.paid_at), format_eur(p.amount_cents)));
                        if !p.note.is_empty() {
                            widgets::hint(ui, p.note.as_str());
                        }
                        if ui.small_button("×").on_hover_text("Zahlung entfernen").clicked() {
                            *act = Act::DeletePayment(p.id);
                        }
                    });
                }
                if state == Some(InvoiceState::Open) {
                    ui.separator();
                    ui.horizontal(|ui| {
                        widgets::date_edit(ui, "pay_date", &mut self.new_payment.paid_at);
                        widgets::eur_edit(ui, &mut self.new_payment.amount_cents);
                        ui.add(egui::TextEdit::singleline(&mut self.new_payment.note).desired_width(160.0).hint_text("Notiz"));
                        if widgets::primary_button(ui, "Zahlung erfassen").clicked() {
                            *act = Act::AddPayment;
                        }
                    });
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
