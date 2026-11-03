use druckplaner_core::{
    board::BoardColumn,
    date::format_de,
    money::format_eur,
    orders::{OrderStatus, OrderSummary},
    overview::OpenItems,
    stock::StockKey,
    Db,
};
use eframe::egui;

use crate::widgets;

/// Sprungziel aus der Übersicht in eine andere Seite.
pub enum Nav {
    Order(i64),
    Invoice(i64),
    Stock(StockKey),
}

/// Verschieben einer Karte auf dem Board, noch nicht bestätigt.
#[derive(Clone)]
struct PendingMove {
    order_id: i64,
    customer: String,
    target: BoardColumn,
}

#[derive(Default)]
pub struct Overview {
    items: OpenItems,
    /// Alle noch nicht bezahlten Bestellungen
    board: Vec<OrderSummary>,
    today: String,
    pending: Option<PendingMove>,
}

/// Eine Karte: oben der Griff zum Ziehen (Infotext), darunter die Knöpfe – die liegen bewusst außerhalb
/// des Zieh-Bereichs, damit sie klickbar bleiben.
fn board_card(
    ui: &mut egui::Ui,
    s: &OrderSummary,
    col: BoardColumn,
    today: &str,
    error: egui::Color32,
    nav: &mut Option<Nav>,
    mv: &mut Option<(i64, BoardColumn)>,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.dnd_drag_source(egui::Id::new(("board_card", s.id)), s.id, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(egui::RichText::new(&s.customer).strong()).truncate());
            ui.label(format!("{} Stk. · {}", s.ordered_qty, format_eur(s.total_cents)));
            let marker = match s.status() {
                OrderStatus::Packed => Some("verpackt".to_string()),
                OrderStatus::PartlyShipped => Some(format!("teilweise versendet ({}/{})", s.shipped_qty, s.ordered_qty)),
                OrderStatus::Shipped => Some("versendet – Rechnung fehlt".to_string()),
                OrderStatus::Invoiced => Some("Zahlung offen".to_string()),
                _ => None,
            };
            if let Some(text) = marker {
                ui.colored_label(ui.visuals().warn_fg_color, text);
            }
            ui.weak(format!("Eingang {}", format_de(&s.received_at)));
            if !s.due_date.is_empty() {
                let text = format!("Termin {}", format_de(&s.due_date));
                if s.due_date.as_str() < today && s.remaining_qty() > 0 {
                    ui.colored_label(error, text);
                } else {
                    ui.label(text);
                }
            }
        });
        ui.horizontal(|ui| {
            // Zurücknehmen einer Rechnung geht nur per Storno, daher kein „zurück“ ab „Rechnung gestellt“
            let prev = col.prev().filter(|_| col != BoardColumn::Invoiced);
            if ui.add_enabled(prev.is_some(), egui::Button::new("‹").small()).on_hover_text("Eine Spalte zurück").clicked() {
                *mv = prev.map(|c| (s.id, c));
            }
            if ui.small_button("Öffnen").clicked() {
                *nav = Some(Nav::Order(s.id));
            }
            let next = col.next();
            if ui.add_enabled(next.is_some(), egui::Button::new("›").small()).on_hover_text("Eine Spalte weiter").clicked() {
                *mv = next.map(|c| (s.id, c));
            }
        });
    });
}

impl Overview {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(items) = widgets::report(status, db.open_items(), "") {
            self.items = items;
        }
        if let Some(all) = widgets::report(status, db.list_order_summaries(), "") {
            let mut board: Vec<OrderSummary> = all.into_iter().filter(|s| s.board_column().is_some()).collect();
            // dringendste zuerst: Termin, sonst Eingang
            board.sort_by(|a, b| {
                (a.due_date.is_empty(), &a.due_date, &a.received_at).cmp(&(b.due_date.is_empty(), &b.due_date, &b.received_at))
            });
            self.board = board;
        }
        if let Some(today) = widgets::report(status, db.today(), "") {
            self.today = today;
        }
    }

    /// Führt das Verschieben aus; „Rechnung gestellt“ erst nach Bestätigung (erzeugt nummerierte Rechnungen).
    fn request_move(&mut self, db: &Db, status: &mut String, order_id: i64, target: BoardColumn) {
        if target == BoardColumn::Invoiced {
            let customer = self.board.iter().find(|s| s.id == order_id).map(|s| s.customer.clone()).unwrap_or_default();
            self.pending = Some(PendingMove { order_id, customer, target });
            return;
        }
        self.apply_move(db, status, order_id, target);
    }

    fn apply_move(&mut self, db: &Db, status: &mut String, order_id: i64, target: BoardColumn) {
        if let Some(msg) = widgets::report(status, db.move_order(order_id, target), "") {
            if !msg.is_empty() {
                *status = msg;
            }
        }
        self.reload(db, status);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) -> Option<Nav> {
        let mut nav = None;
        let mut create_invoice = None;
        let mut mv: Option<(i64, BoardColumn)> = None;
        let items = &self.items;
        let board = &self.board;
        let today = self.today.as_str();
        let error = ui.visuals().error_fg_color;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Was ist offen?");
            let owed: i64 = items.to_collect.iter().map(|i| i.open_cents).sum();
            ui.label(format!(
                "{} Bestellung(en) in Arbeit · {} zu versenden · {} Rechnung(en) fehlen · offene Forderungen {}",
                items.orders.len(),
                items.to_ship.len(),
                items.to_invoice.len(),
                format_eur(owed)
            ));

            ui.add_space(12.0);
            ui.strong("Bestellungen – noch nicht vollständig versendet");
            if items.orders.is_empty() {
                ui.weak("Nichts offen.");
            }
            egui::Grid::new("open_orders").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for o in &items.orders {
                    ui.label(&o.customer);
                    ui.label(format!("Eingang {}", format_de(&o.received_at)));
                    ui.label(if o.due_date.is_empty() { "kein Termin".to_string() } else { format!("Termin {}", format_de(&o.due_date)) });
                    ui.label(format!("{} Stk. offen", o.remaining_qty));
                    ui.label(o.status.label());
                    if ui.button("Öffnen").clicked() {
                        nav = Some(Nav::Order(o.order_id));
                    }
                    ui.end_row();
                }
            });

            ui.add_space(12.0);
            ui.strong("Board – Karten ziehen oder mit ‹ › verschieben");
            ui.columns(BoardColumn::ALL.len(), |columns| {
                for (idx, ui) in columns.iter_mut().enumerate() {
                    let col = BoardColumn::ALL[idx];
                    let cards: Vec<&OrderSummary> = board.iter().filter(|s| s.board_column() == Some(col)).collect();
                    ui.strong(format!("{} ({})", col.label(), cards.len()));
                    let frame = egui::Frame::group(ui.style());
                    let (_, dropped) = ui.dnd_drop_zone::<i64, ()>(frame, |ui| {
                        ui.set_min_size(egui::vec2(ui.available_width(), 110.0));
                        for s in &cards {
                            board_card(ui, s, col, today, error, &mut nav, &mut mv);
                            ui.add_space(4.0);
                        }
                    });
                    if let Some(id) = dropped {
                        if board.iter().any(|s| s.id == *id && s.board_column() != Some(col)) {
                            mv = Some((*id, col));
                        }
                    }
                }
            });


            ui.add_space(12.0);
            ui.strong("Verpackt – noch nicht versendet");
            if items.to_ship.is_empty() {
                ui.weak("Nichts offen.");
            }
            egui::Grid::new("open_to_ship").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for s in &items.to_ship {
                    ui.label(&s.customer);
                    ui.label(format!("gepackt {}", format_de(&s.packed_at)));
                    ui.label(format_eur(s.amount_cents));
                    if ui.button("Öffnen").clicked() {
                        nav = Some(Nav::Order(s.order_id));
                    }
                    ui.end_row();
                }
            });

            ui.add_space(12.0);
            ui.strong("Versendet – Rechnung fehlt");
            if items.to_invoice.is_empty() {
                ui.weak("Nichts offen.");
            }
            egui::Grid::new("open_to_invoice").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for s in &items.to_invoice {
                    ui.label(&s.customer);
                    ui.label(format!("versendet {}", format_de(&s.shipped_at)));
                    ui.label(format_eur(s.amount_cents));
                    if ui.button("Rechnung erstellen").clicked() {
                        create_invoice = Some(s.shipment_id);
                    }
                    if ui.button("Bestellung").clicked() {
                        nav = Some(Nav::Order(s.order_id));
                    }
                    ui.end_row();
                }
            });

            ui.add_space(12.0);
            ui.strong("Rechnungen – Zahlung offen");
            if items.to_collect.is_empty() {
                ui.weak("Nichts offen.");
            }
            egui::Grid::new("open_to_collect").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for i in &items.to_collect {
                    ui.label(&i.number);
                    ui.label(&i.customer);
                    ui.label(format!("vom {}", format_de(&i.issued_at)));
                    ui.label(format_eur(i.open_cents));
                    if i.overdue_days > 0 {
                        ui.colored_label(error, format!("{} Tage überfällig", i.overdue_days));
                    } else {
                        ui.label(format!("fällig {}", format_de(&i.due_at)));
                    }
                    if ui.button("Öffnen").clicked() {
                        nav = Some(Nav::Invoice(i.invoice_id));
                    }
                    ui.end_row();
                }
            });

            ui.add_space(12.0);
            ui.strong("Nachbestellen / Nachdrucken – Lager unter Mindestbestand oder knapp");
            if items.low_stock.is_empty() {
                ui.weak("Nichts zu tun.");
            }
            egui::Grid::new("open_low_stock").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                for l in &items.low_stock {
                    ui.label(l.name.as_str());
                    ui.label(format!("{} {}", l.qty, l.unit));
                    if l.below_min() {
                        ui.colored_label(error, format!("unter Mindestbestand ({} {})", l.min_qty, l.unit));
                    } else if let Some(days) = l.days_left() {
                        ui.label(format!("reicht ca. {days} Tage, Lieferzeit {} Tage", l.lead_time_days));
                    } else {
                        ui.label("");
                    }
                    if ui.button("Lager").clicked() {
                        nav = Some(Nav::Stock(l.key));
                    }
                    ui.end_row();
                }
            });
        });

        if let Some((order_id, target)) = mv {
            self.request_move(db, status, order_id, target);
        }
        if let Some(pending) = self.pending.clone() {
            let mut decision = None;
            egui::Window::new("Rechnung erstellen?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!(
                        "Für die Bestellung von {} werden jetzt Rechnungen erstellt. Noch nicht versendete \
                         Lieferungen gelten dabei als heute versendet.",
                        pending.customer
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Rechnung erstellen").clicked() {
                            decision = Some(true);
                        }
                        if ui.button("Abbrechen").clicked() {
                            decision = Some(false);
                        }
                    });
                });
            match decision {
                Some(true) => {
                    self.pending = None;
                    self.apply_move(db, status, pending.order_id, pending.target);
                }
                Some(false) => self.pending = None,
                None => {}
            }
        }
        if let Some(shipment_id) = create_invoice {
            let result = db.create_invoice(shipment_id).and_then(|id| db.load_invoice(id));
            if let Some(inv) = widgets::report(status, result, "") {
                *status = format!("Rechnung {} erstellt", inv.number);
                widgets::open_file(&db.invoice_pdf_file(&inv), status);
                self.reload(db, status);
                nav = Some(Nav::Invoice(inv.id));
            }
        }
        nav
    }
}
