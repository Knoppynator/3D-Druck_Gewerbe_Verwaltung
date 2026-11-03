use druckplaner_core::{
    dashboard::{CostRates, MonthSales, ProductCost, Projection, ReachRow, Tiles},
    date::format_de,
    money::format_eur,
    Db,
};
use eframe::egui::{self, Color32};

use crate::widgets;

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];

#[derive(Default)]
pub struct DashboardView {
    tiles: Tiles,
    months: Vec<MonthSales>,
    projection: Projection,
    reach: Vec<ReachRow>,
    costs: Vec<ProductCost>,
    rates: CostRates,
}

/// `2026-04` → `Apr 26`.
fn month_label(ym: &str) -> String {
    let month: usize = ym.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!("{} {}", MONTHS[month.clamp(1, 12) - 1], ym.get(2..4).unwrap_or(""))
}

fn eur(value: f64) -> String {
    format_eur((value * 100.0).round() as i64)
}

fn tile(ui: &mut egui::Ui, title: &str, value: String, sub: Option<(String, Color32)>) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(160.0);
        ui.vertical(|ui| {
            ui.weak(title);
            ui.label(egui::RichText::new(value).size(20.0).strong());
            match sub {
                Some((text, color)) => ui.colored_label(color, text),
                None => ui.label(""),
            };
        });
    });
}

/// Einfaches Säulendiagramm; der letzte (laufende) Monat ist blasser, weil er noch nicht vollständig ist.
fn bar_chart(ui: &mut egui::Ui, data: &[(String, f64)], color: Color32, format_value: &dyn Fn(f64) -> String) {
    let height = 150.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height + 36.0), egui::Sense::hover());
    if data.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    let max = data.iter().map(|d| d.1).fold(0.0, f64::max).max(1.0);
    let slot = rect.width() / data.len() as f32;
    let bar_width = slot * 0.62;
    let base = rect.top() + 18.0 + height;
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();

    painter.line_segment([egui::pos2(rect.left(), base), egui::pos2(rect.right(), base)], egui::Stroke::new(1.0_f32, weak));
    for (i, (label, value)) in data.iter().enumerate() {
        let x = rect.left() + slot * i as f32 + (slot - bar_width) / 2.0;
        let h = (value.max(0.0) / max) as f32 * height;
        let fill = if i + 1 == data.len() { color.gamma_multiply(0.5) } else { color };
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x, base - h), egui::pos2(x + bar_width, base)), 2.0, fill);
        if *value != 0.0 {
            painter.text(
                egui::pos2(x + bar_width / 2.0, base - h - 2.0),
                egui::Align2::CENTER_BOTTOM,
                format_value(*value),
                egui::FontId::proportional(10.0),
                text_color,
            );
        }
        painter.text(egui::pos2(x + bar_width / 2.0, base + 4.0), egui::Align2::CENTER_TOP, label, egui::FontId::proportional(10.0), weak);
    }
    if let Some(pos) = response.hover_pos() {
        let i = (((pos.x - rect.left()) / slot) as usize).min(data.len() - 1);
        response.on_hover_text(format!("{}: {}", data[i].0, format_value(data[i].1)));
    }
}

impl DashboardView {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.dashboard_tiles(), "") {
            self.tiles = v;
        }
        if let Some(v) = widgets::report(status, db.monthly_sales(12), "") {
            self.months = v;
        }
        if let Some(v) = widgets::report(status, db.projection(), "") {
            self.projection = v;
        }
        if let Some(v) = widgets::report(status, db.reach_rows(), "") {
            self.reach = v;
        }
        if let Some(v) = widgets::report(status, db.product_costing(), "") {
            self.costs = v;
        }
        if let Some(v) = widgets::report(status, db.cost_rates(), "") {
            self.rates = v;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let error = ui.visuals().error_fg_color;
        let warn = ui.visuals().warn_fg_color;
        let t = &self.tiles;
        let p = &self.projection;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Dashboard");
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                tile(ui, &format!("Umsatz {}", t.year), format_eur(t.revenue_cents), Some(("nach Rechnungsdatum".into(), ui.visuals().weak_text_color())));
                tile(
                    ui,
                    &format!("Gewinn {} (EÜR)", t.year),
                    format_eur(t.profit_cents),
                    Some((format!("{} ein − {} aus", format_eur(t.income_cents), format_eur(t.expenses_cents)), ui.visuals().weak_text_color())),
                );
                tile(
                    ui,
                    "Offene Forderungen",
                    format_eur(t.receivables_cents),
                    Some(if t.overdue_cents > 0 {
                        (format!("{} überfällig", format_eur(t.overdue_cents)), error)
                    } else {
                        ("nichts überfällig".into(), ui.visuals().weak_text_color())
                    }),
                );
                tile(
                    ui,
                    "Offene Bestellungen",
                    format!("{} Stk.", t.open_units),
                    Some((format!("{} Bestellung(en)", t.open_orders), ui.visuals().weak_text_color())),
                );
                tile(
                    ui,
                    "Druck-Warteschlange",
                    if t.queue_ends_at.is_empty() { "leer".to_string() } else { druckplaner_core::date::format_datetime_de(&t.queue_ends_at) },
                    Some(("voraussichtlich fertig".into(), ui.visuals().weak_text_color())),
                );
                tile(
                    ui,
                    "Nachbestellen",
                    t.low_stock.to_string(),
                    Some(if t.low_stock > 0 { ("Posten unter Mindestbestand/knapp".into(), warn) } else { ("alles im Soll".into(), ui.visuals().weak_text_color()) }),
                );
            });

            // ---- Umsatz und Absatz
            ui.add_space(14.0);
            ui.strong("Umsatz und Absatz der letzten 12 Monate");
            let revenue: Vec<(String, f64)> = self.months.iter().map(|m| (month_label(&m.month), m.revenue_cents as f64 / 100.0)).collect();
            let units: Vec<(String, f64)> = self.months.iter().map(|m| (month_label(&m.month), m.units as f64)).collect();
            ui.columns(2, |cols| {
                cols[0].label("Umsatz je Monat (Rechnungsdatum, inkl. Stornos)");
                bar_chart(&mut cols[0], &revenue, Color32::from_rgb(52, 120, 200), &|v| format!("{v:.0} €"));
                cols[1].label("Verkaufte Sets je Monat");
                bar_chart(&mut cols[1], &units, Color32::from_rgb(70, 160, 110), &|v| format!("{v:.0}"));
            });
            ui.add_space(4.0);
            match (p.revenue_projected_cents, p.units_projected) {
                (Some(revenue), Some(units)) => {
                    ui.label(format!(
                        "Hochrechnung {}: Umsatz {} und {} Sets (bisher {} und {} Sets in {} Monaten).",
                        p.year,
                        format_eur(revenue),
                        units,
                        format_eur(p.revenue_ytd_cents),
                        p.units_ytd,
                        widgets::dec(p.months_elapsed, 1)
                    ));
                }
                _ => {
                    ui.label(format!("Hochrechnung {}: erst nach dem ersten vollen Monat möglich.", p.year));
                }
            }
            ui.label(format!("Letzte 12 Monate: Umsatz {} und {} Sets.", format_eur(p.revenue_last12_cents), p.units_last12));
            ui.weak("Die Hochrechnung rechnet den bisherigen Jahreswert linear auf 12 Monate hoch – bei Bestellungen in größeren Posten schwankt sie stark.");
            if p.unknown_units_invoices > 0 {
                ui.colored_label(
                    warn,
                    format!(
                        "Bei {} importierten Altrechnungen fehlt die Stückzahl (unter Rechnungen nachtragen). Bis dahin ist der Absatz zu niedrig.",
                        p.unknown_units_invoices
                    ),
                );
            }

            // ---- Reichweite
            ui.add_space(14.0);
            ui.strong("Materialreichweite");
            if self.reach.is_empty() {
                ui.weak("Noch kein Verbrauch in den letzten 90 Tagen erfasst. Verbrauch entsteht durch versendete Lieferungen (Zukaufteile, Fertigteile) und abgeschlossene Druckjobs (Filament).");
            } else {
                egui::Grid::new("dash_reach").striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                    for h in ["Posten", "Bestand", "Verbrauch (90 Tage)", "Reicht noch", "Lieferzeit", "Bestellen bis"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for r in &self.reach {
                        ui.label(r.name.as_str());
                        ui.label(format!("{} {}", r.qty, r.unit));
                        ui.label(format!("{} {}", r.consumed_90d, r.unit));
                        ui.label(format!("{} Tage", r.days_left));
                        ui.label(if r.lead_time_days > 0 { format!("{} Tage", r.lead_time_days) } else { "–".to_string() });
                        if r.overdue {
                            ui.colored_label(error, "jetzt bestellen");
                        } else {
                            ui.label(format_de(&r.order_by));
                        }
                        ui.end_row();
                    }
                });
            }

            // ---- Marge je Produkt
            ui.add_space(14.0);
            ui.strong("Marge je Produkt");
            ui.weak(format!(
                "Sätze: Maschine {}/h, Arbeitszeit {}/h, Filament {}/kg (Durchschnitt der hinterlegten Preise).",
                eur(self.rates.machine_eur_per_h),
                eur(self.rates.labor_eur_per_h),
                eur(self.rates.filament_eur_per_g * 1000.0)
            ));
            if self.costs.is_empty() {
                ui.weak("Noch keine Produkte angelegt.");
            }
            for c in &self.costs {
                egui::CollapsingHeader::new(c.name.as_str()).id_salt(("cost", c.product_id)).default_open(true).show(ui, |ui| {
                    egui::Grid::new(("cost_grid", c.product_id)).spacing([16.0, 3.0]).show(ui, |ui| {
                        ui.label("Zukaufteile");
                        ui.label(eur(c.purchase_eur));
                        ui.end_row();
                        ui.label(format!("Filament ({} g)", widgets::dec(c.filament_g, 0)));
                        ui.label(eur(c.filament_eur));
                        ui.end_row();
                        ui.label(format!("Druckzeit ({} h)", widgets::dec(c.print_hours, 2)));
                        ui.label(eur(c.machine_eur));
                        ui.end_row();
                        ui.strong("Selbstkosten ohne Arbeitszeit");
                        ui.strong(eur(c.cost_before_labor()));
                        ui.end_row();
                        ui.label("Arbeitszeit");
                        ui.label(eur(c.labor_eur));
                        ui.end_row();
                        ui.label("Selbstkosten mit Arbeitszeit");
                        ui.label(eur(c.cost_total()));
                        ui.end_row();
                    });
                    if !c.tiers.is_empty() {
                        ui.add_space(4.0);
                        egui::Grid::new(("tier_grid", c.product_id)).striped(true).spacing([16.0, 3.0]).show(ui, |ui| {
                            for h in ["Staffel", "Preis", "Gewinn vor Arbeit", "Marge", "Gewinn nach Arbeit"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (qty, price) in &c.tiers {
                                let before = c.profit_before_labor(*price);
                                let after = c.profit_after_labor(*price);
                                ui.label(format!("ab {qty} Stk."));
                                ui.label(format_eur(*price));
                                if before < 0.0 {
                                    ui.colored_label(error, eur(before));
                                } else {
                                    ui.label(eur(before));
                                }
                                ui.label(c.margin_pct(*price).map_or("–".to_string(), |m| format!("{m:.0} %")));
                                if after < 0.0 {
                                    ui.colored_label(warn, eur(after));
                                } else {
                                    ui.label(eur(after));
                                }
                                ui.end_row();
                            }
                        });
                    }
                    for w in &c.warnings {
                        ui.colored_label(warn, w.as_str());
                    }
                });
            }
            ui.weak("Druckzeit und Filament je Teil stammen aus der Plattenvorlage mit den meisten Exemplaren und werden gleichmäßig auf alle Stücke der Platte verteilt. Versandkosten sind nicht enthalten.");
        });
    }
}
