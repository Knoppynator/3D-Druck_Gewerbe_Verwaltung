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

use crate::{
    icons::{self, Icon},
    theme,
    widgets::{self, Stat, Tone},
};

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
fn board_card(ui: &mut egui::Ui, s: &OrderSummary, col: BoardColumn, today: &str, nav: &mut Option<Nav>, mv: &mut Option<(i64, BoardColumn)>) {
    let c = theme::p();
    widgets::card_frame().inner_margin(10).corner_radius(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.dnd_drag_source(egui::Id::new(("board_card", s.id)), s.id, |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add(egui::Label::new(theme::strong(&s.customer, 14.0)).truncate());
            ui.label(egui::RichText::new(format!("{} Stk. · {}", s.ordered_qty, format_eur(s.total_cents))).size(13.0).color(c.text_soft));
            let marker = match s.status() {
                OrderStatus::Packed => Some((Tone::Neutral, "verpackt".to_string())),
                OrderStatus::PartlyShipped => Some((Tone::Accent, format!("teilweise versendet ({}/{})", s.shipped_qty, s.ordered_qty))),
                OrderStatus::Shipped => Some((Tone::Warn, "Rechnung fehlt".to_string())),
                OrderStatus::Invoiced => Some((Tone::Warn, "Zahlung offen".to_string())),
                _ => None,
            };
            if let Some((tone, text)) = marker {
                widgets::chip(ui, tone, &text);
            }
            if !s.due_date.is_empty() {
                if s.due_date.as_str() < today && s.remaining_qty() > 0 {
                    widgets::chip(ui, Tone::Critical, "Termin überschritten");
                    widgets::hint(ui, &format!("Termin war {}", format_de(&s.due_date)));
                } else {
                    widgets::hint(ui, &format!("Termin {}", format_de(&s.due_date)));
                }
            } else {
                widgets::hint(ui, &format!("Eingang {}", format_de(&s.received_at)));
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
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

/// Tabelle in einer Karte; leere Listen zeigen einen ruhigen Hinweis.
fn list_card(ui: &mut egui::Ui, title: &str, count: usize, empty: &str, add: impl FnOnce(&mut egui::Ui)) {
    widgets::card(ui, title, &if count == 0 { String::new() } else { count.to_string() }, |ui| {
        if count == 0 {
            ui.horizontal(|ui| {
                icons::show(ui, Icon::Check, 16.0, theme::p().good_text);
                widgets::hint(ui, empty);
            });
        } else {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            egui::ScrollArea::horizontal().id_salt(title).show(ui, add);
        }
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

        egui::ScrollArea::vertical().show(ui, |ui| {
            let owed: i64 = items.to_collect.iter().map(|i| i.open_cents).sum();
            let overdue = items.to_collect.iter().filter(|i| i.overdue_days > 0).count();
            widgets::page_header(ui, "Was ist offen?", "Alles, was als Nächstes zu tun ist – von der Bestellung bis zur Zahlung", |_| {});
            let stats = [
                Stat::new("In Arbeit", items.orders.len().to_string(), "noch nicht ganz versendet"),
                Stat::new("Zu versenden", items.to_ship.len().to_string(), "verpackte Lieferungen")
                    .tone(if items.to_ship.is_empty() { Tone::Neutral } else { Tone::Accent }),
                Stat::new("Rechnung fehlt", items.to_invoice.len().to_string(), "versendete Lieferungen")
                    .tone(if items.to_invoice.is_empty() { Tone::Neutral } else { Tone::Warn }),
                Stat::new("Forderungen", format_eur(owed), if overdue > 0 { format!("{overdue} Rechnung(en) überfällig") } else { "nichts überfällig".into() })
                    .tone(if overdue > 0 { Tone::Critical } else { Tone::Neutral }),
                Stat::new("Nachbestellen", items.low_stock.len().to_string(), "Posten knapp")
                    .tone(if items.low_stock.is_empty() { Tone::Neutral } else { Tone::Warn }),
            ];
            widgets::stat_rows(ui, &stats, 180.0);

            widgets::card(ui, "Board", "Karten ziehen oder mit ‹ › verschieben", |ui| {
                let c = theme::p();
                ui.columns(BoardColumn::ALL.len(), |columns| {
                    for (idx, ui) in columns.iter_mut().enumerate() {
                        let col = BoardColumn::ALL[idx];
                        let cards: Vec<&OrderSummary> = board.iter().filter(|s| s.board_column() == Some(col)).collect();
                        ui.horizontal(|ui| {
                            ui.label(theme::strong(col.label(), 13.0));
                            ui.label(egui::RichText::new(cards.len().to_string()).size(12.0).color(c.muted));
                        });
                        let frame = egui::Frame::new().fill(c.faint).stroke(egui::Stroke::new(1.0_f32, c.line_soft)).corner_radius(10).inner_margin(6);
                        let (_, dropped) = ui.dnd_drop_zone::<i64, ()>(frame, |ui| {
                            ui.set_min_size(egui::vec2(ui.available_width(), 120.0));
                            for s in &cards {
                                board_card(ui, s, col, today, &mut nav, &mut mv);
                                ui.add_space(2.0);
                            }
                        });
                        if let Some(id) = dropped {
                            if board.iter().any(|s| s.id == *id && s.board_column() != Some(col)) {
                                mv = Some((*id, col));
                            }
                        }
                    }
                });
            });

            list_card(ui, "Bestellungen in Arbeit", items.orders.len(), "Keine offenen Bestellungen.", |ui| {
                egui::Grid::new("open_orders").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                    widgets::table_head(ui, &["Kunde", "Eingang", "Termin", "Offen", "Stand", ""]);
                    for o in &items.orders {
                        ui.label(theme::strong(&o.customer, 14.0));
                        ui.label(format_de(&o.received_at));
                        if o.due_date.is_empty() {
                            widgets::hint(ui, "kein Termin");
                        } else if o.due_date.as_str() < today {
                            widgets::chip(ui, Tone::Critical, &format!("{} überschritten", format_de(&o.due_date)));
                        } else {
                            ui.label(format_de(&o.due_date));
                        }
                        ui.label(format!("{} Stk.", o.remaining_qty));
                        widgets::chip(ui, Tone::Accent, o.status.label());
                        if ui.small_button("Öffnen").clicked() {
                            nav = Some(Nav::Order(o.order_id));
                        }
                        ui.end_row();
                    }
                });
            });

            let (mut nav_a, mut nav_b) = (None, None);
            widgets::two_up(
                ui,
                1000.0,
                |ui| {
                    list_card(ui, "Verpackt – noch nicht versendet", items.to_ship.len(), "Nichts zu versenden.", |ui| {
                        egui::Grid::new("open_to_ship").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                            for s in &items.to_ship {
                                ui.label(theme::strong(&s.customer, 14.0));
                                ui.label(format!("gepackt {}", format_de(&s.packed_at)));
                                ui.label(format_eur(s.amount_cents));
                                if ui.small_button("Öffnen").clicked() {
                                    nav_a = Some(Nav::Order(s.order_id));
                                }
                                ui.end_row();
                            }
                        });
                    });
                },
                |ui| {
                    list_card(ui, "Versendet – Rechnung fehlt", items.to_invoice.len(), "Alle Lieferungen sind abgerechnet.", |ui| {
                        egui::Grid::new("open_to_invoice").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                            for s in &items.to_invoice {
                                ui.label(theme::strong(&s.customer, 14.0));
                                ui.label(format!("versendet {}", format_de(&s.shipped_at)));
                                ui.label(format_eur(s.amount_cents));
                                ui.horizontal(|ui| {
                                    if widgets::soft_button(ui, "Rechnung erstellen").clicked() {
                                        create_invoice = Some(s.shipment_id);
                                    }
                                    if ui.small_button("Bestellung").clicked() {
                                        nav_b = Some(Nav::Order(s.order_id));
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    });
                },
            );

            nav = nav.take().or(nav_a.take()).or(nav_b.take());
            widgets::two_up(
                ui,
                1000.0,
                |ui| {
                    list_card(ui, "Rechnungen – Zahlung offen", items.to_collect.len(), "Alle Rechnungen sind bezahlt.", |ui| {
                        egui::Grid::new("open_to_collect").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                            for i in &items.to_collect {
                                ui.label(theme::strong(&i.number, 14.0));
                                ui.label(&i.customer);
                                ui.label(format_eur(i.open_cents));
                                if i.overdue_days > 0 {
                                    widgets::chip(ui, Tone::Critical, &format!("{} Tage überfällig", i.overdue_days));
                                } else {
                                    widgets::chip(ui, Tone::Warn, &format!("fällig {}", format_de(&i.due_at)));
                                }
                                if ui.small_button("Öffnen").clicked() {
                                    nav_a = Some(Nav::Invoice(i.invoice_id));
                                }
                                ui.end_row();
                            }
                        });
                    });
                },
                |ui| {
                    list_card(ui, "Nachbestellen / Nachdrucken", items.low_stock.len(), "Alle Bestände im Soll.", |ui| {
                        egui::Grid::new("open_low_stock").striped(true).spacing([16.0, 6.0]).show(ui, |ui| {
                            for l in &items.low_stock {
                                ui.label(theme::strong(l.name.as_str(), 14.0));
                                ui.label(format!("{} {}", l.qty, l.unit));
                                if l.below_min() {
                                    widgets::chip(ui, Tone::Critical, &format!("unter Minimum ({} {})", l.min_qty, l.unit));
                                } else if let Some(days) = l.days_left() {
                                    widgets::chip(ui, Tone::Warn, &format!("reicht ca. {days} Tage"));
                                } else {
                                    ui.label("");
                                }
                                if ui.small_button("Lager").clicked() {
                                    nav_b = Some(Nav::Stock(l.key));
                                }
                                ui.end_row();
                            }
                        });
                    });
                },
            );
            nav = nav.take().or(nav_a).or(nav_b);
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
                        if widgets::primary_button(ui, "Rechnung erstellen").clicked() {
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
