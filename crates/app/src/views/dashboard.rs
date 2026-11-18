use druckplaner_core::{
    dashboard::{CostRates, MonthSales, ProductCost, Projection, ReachRow, Tiles},
    date::{format_datetime_de, format_de},
    money::format_eur,
    Db,
};
use eframe::egui::{self, Color32};

use crate::{
    theme,
    widgets::{self, Stat, Tone},
};

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"];

/// Skala der Reichweite-Balken in Tagen (alles darüber ist „voll“).
const REACH_SCALE_DAYS: f32 = 90.0;

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

/// Ganze Euro ohne Cent für Achsen und Beschriftungen, z. B. `1.250 €`.
fn eur_short(value: f64) -> String {
    let s = format_eur((value.round() * 100.0) as i64);
    s.trim_end_matches(" €").trim_end_matches(",00").to_string() + " €"
}

/// Runder Achsen-Höchstwert und Schrittweite für `max` (1-2-5-Reihe, 3–5 Linien).
fn nice_axis(max: f64) -> (f64, f64) {
    let max = max.max(1.0);
    let raw = max / 4.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|f| f * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    ((max / step).ceil() * step, step)
}

/// Säulendiagramm nach Dataviz-Regeln: eine Achse, feine Gitterlinien, Säulen höchstens 24 px breit und oben
/// gerundet, Beschriftung nur am Höchstwert und am laufenden (helleren) Monat, alles andere im Tooltip.
fn bar_chart(ui: &mut egui::Ui, data: &[(String, f64)], format_value: &dyn Fn(f64) -> String) {
    let c = theme::p();
    let plot_h = 170.0;
    let axis_w = 54.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), plot_h + 40.0), egui::Sense::hover());
    if data.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    let (top, step) = nice_axis(data.iter().map(|d| d.1).fold(0.0, f64::max));
    let plot = egui::Rect::from_min_max(egui::pos2(rect.left() + axis_w, rect.top() + 18.0), egui::pos2(rect.right(), rect.top() + 18.0 + plot_h));
    let y_of = |v: f64| plot.bottom() - (v / top) as f32 * plot.height();
    let small = egui::FontId::proportional(11.0);

    // Gitter und Achse
    let mut v = 0.0;
    while v <= top + 1e-9 {
        let y = y_of(v);
        let stroke = if v == 0.0 { egui::Stroke::new(1.0_f32, c.field_line) } else { egui::Stroke::new(1.0_f32, c.line_soft) };
        painter.line_segment([egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)], stroke);
        painter.text(egui::pos2(plot.left() - 8.0, y), egui::Align2::RIGHT_CENTER, format_value(v), small.clone(), c.muted);
        v += step;
    }

    let slot = plot.width() / data.len() as f32;
    let bar_w = (slot * 0.6).min(24.0);
    let max_value = data.iter().map(|d| d.1).fold(f64::MIN, f64::max);
    let hovered = response.hover_pos().filter(|p| plot.x_range().contains(p.x)).map(|p| (((p.x - plot.left()) / slot) as usize).min(data.len() - 1));
    for (i, (label, value)) in data.iter().enumerate() {
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let current = i + 1 == data.len();
        let mut fill = if current { c.accent_light } else { c.accent };
        if hovered.is_some_and(|h| h != i) {
            fill = fill.gamma_multiply(0.55);
        }
        if *value > 0.0 {
            let bar = egui::Rect::from_min_max(egui::pos2(cx - bar_w / 2.0, y_of(*value)), egui::pos2(cx + bar_w / 2.0, plot.bottom()));
            painter.rect_filled(bar, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, fill);
            if *value == max_value || current {
                let font = egui::FontId::new(11.0, theme::semibold());
                painter.text(egui::pos2(cx, bar.top() - 3.0), egui::Align2::CENTER_BOTTOM, format_value(*value), font, c.text);
            }
        }
        painter.text(egui::pos2(cx, plot.bottom() + 6.0), egui::Align2::CENTER_TOP, label, small.clone(), c.muted);
    }
    if let Some(i) = hovered {
        let note = if i + 1 == data.len() { " (Monat läuft noch)" } else { "" };
        response.on_hover_text(format!("{}: {}{note}", data[i].0, format_value(data[i].1)));
    }
}

/// Kleine Verlaufslinie (grau, letzter Abschnitt im Akzent mit Endpunkt).
fn sparkline(ui: &mut egui::Ui, values: &[f64]) {
    let c = theme::p();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
    if values.len() < 2 {
        return;
    }
    let max = values.iter().copied().fold(0.0, f64::max).max(1.0);
    let pts: Vec<egui::Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (values.len() - 1) as f32;
            egui::pos2(x, rect.bottom() - 4.0 - (*v / max) as f32 * (rect.height() - 8.0))
        })
        .collect();
    let painter = ui.painter();
    painter.add(egui::Shape::line(pts[..pts.len() - 1].to_vec(), egui::Stroke::new(2.0_f32, c.field_line)));
    painter.line_segment([pts[pts.len() - 2], pts[pts.len() - 1]], egui::Stroke::new(2.0_f32, c.accent));
    let end = pts[pts.len() - 1];
    painter.circle(end, 4.0, c.accent, egui::Stroke::new(2.0_f32, c.card));
}

/// Kostenbalken: ein Ganzes, in Teile geteilt (Zukauf, Filament, Druckzeit, Arbeit); Beträge stehen in den Teilen,
/// wo sie hineinpassen.
fn cost_bar(ui: &mut egui::Ui, parts: &[(f64, &str)]) {
    let c = theme::p();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let total: f64 = parts.iter().map(|p| p.0.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let painter = ui.painter();
    let gap = 2.0;
    let usable = rect.width() - gap * (parts.len() - 1) as f32;
    let mut x = rect.left();
    let font = egui::FontId::new(12.0, theme::semibold());
    for (i, (value, _)) in parts.iter().enumerate() {
        let w = (value.max(0.0) / total) as f32 * usable;
        if w <= 0.0 {
            continue;
        }
        let block = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, rect.height()));
        painter.rect_filled(block, 4.0, c.series[i]);
        // Gelb (Slot 4) trägt dunkle Schrift, die anderen weiße
        let ink = if i == 3 { Color32::from_rgb(0x0b, 0x0b, 0x0b) } else { Color32::WHITE };
        let galley = painter.layout_no_wrap(eur(*value), font.clone(), ink);
        if galley.size().x + 14.0 <= w {
            painter.galley(egui::pos2(block.left() + 7.0, block.center().y - galley.size().y / 2.0), galley, ink);
        }
        x += w + gap;
    }
    response.on_hover_text(parts.iter().map(|(v, name)| format!("{name}: {}", eur(*v))).collect::<Vec<_>>().join("\n"));
}

fn legend(ui: &mut egui::Ui, names: &[String]) {
    ui.horizontal_wrapped(|ui| {
        for (i, name) in names.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, theme::p().series[i]);
            ui.label(egui::RichText::new(name).size(12.0).color(theme::p().text_soft));
            ui.add_space(8.0);
        }
    });
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

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut refresh = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            widgets::page_header(ui, "Dashboard", &format!("Geschäftsjahr {}", self.tiles.year), |ui| {
                if ui.button("Aktualisieren").clicked() {
                    refresh = true;
                }
            });
            self.alerts_ui(ui);
            widgets::two_up(ui, 900.0, |ui| self.hero_ui(ui), |ui| self.stats_ui(ui));
            widgets::two_up(ui, 900.0, |ui| self.chart_ui(ui, true), |ui| self.chart_ui(ui, false));
            if self.projection.unknown_units_invoices > 0 {
                let text = format!(
                    "Bei {} importierten Altrechnungen fehlt die Stückzahl (unter Rechnungen nachtragen) – bis dahin ist der Absatz zu niedrig.",
                    self.projection.unknown_units_invoices
                );
                widgets::alert(ui, Tone::Warn, "", &text, None);
            }
            widgets::two_up(ui, 900.0, |ui| self.reach_ui(ui), |ui| self.margin_ui(ui));
        });
        if refresh {
            self.reload(db, status);
            *status = "Dashboard aktualisiert".into();
        }
    }

    fn alerts_ui(&self, ui: &mut egui::Ui) {
        let urgent: Vec<&str> = self.reach.iter().filter(|r| r.overdue).map(|r| r.name.as_str()).collect();
        if !urgent.is_empty() {
            let mut names = urgent.iter().take(3).copied().collect::<Vec<_>>().join(", ");
            if urgent.len() > 3 {
                names.push_str(&format!(" und {} weitere", urgent.len() - 3));
            }
            widgets::alert(ui, Tone::Warn, "Nachbestellen:", &format!("{names} – jetzt bestellen, sonst reicht der Bestand nicht bis zur Lieferung."), None);
        } else if self.tiles.low_stock > 0 {
            widgets::alert(ui, Tone::Warn, "Nachbestellen:", &format!("{} Posten unter Mindestbestand oder knapp (siehe Lager).", self.tiles.low_stock), None);
        }
    }

    fn hero_ui(&self, ui: &mut egui::Ui) {
        let t = &self.tiles;
        let p = &self.projection;
        widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Umsatz {}", t.year)).color(theme::p().text_soft));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| widgets::hint(ui, "nach Rechnungsdatum"));
            });
            ui.label(theme::strong(format_eur(t.revenue_cents), 48.0));
            let values: Vec<f64> = self.months.iter().map(|m| m.revenue_cents as f64).collect();
            sparkline(ui, &values);
            egui::Grid::new("hero_facts").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                widgets::form_label(ui, "Hochrechnung aufs Jahr");
                match (p.revenue_projected_cents, p.units_projected) {
                    (Some(revenue), Some(units)) => ui.label(theme::strong(format!("{} · {units} Sets", format_eur(revenue)), 14.0)),
                    _ => ui.label(egui::RichText::new("erst nach dem ersten vollen Monat").color(theme::p().muted)),
                };
                ui.end_row();
                widgets::form_label(ui, "Letzte 12 Monate");
                ui.label(theme::strong(format!("{} · {} Sets", format_eur(p.revenue_last12_cents), p.units_last12), 14.0));
                ui.end_row();
            });
            widgets::hint(
                ui,
                &format!(
                    "Bisher {} und {} Sets in {} Monaten, linear hochgerechnet – bei großen Bestellungen schwankt das stark.",
                    format_eur(p.revenue_ytd_cents),
                    p.units_ytd,
                    widgets::dec(p.months_elapsed, 1)
                ),
            );
        });
        ui.add_space(4.0);
    }

    fn stats_ui(&self, ui: &mut egui::Ui) {
        let t = &self.tiles;
        let stats = [
            Stat::new(format!("Gewinn {} (EÜR)", t.year), format_eur(t.profit_cents), format!("{} ein − {} aus", format_eur(t.income_cents), format_eur(t.expenses_cents))),
            if t.overdue_cents > 0 {
                Stat::new("Offene Forderungen", format_eur(t.receivables_cents), format!("{} überfällig", format_eur(t.overdue_cents))).tone(Tone::Critical)
            } else {
                Stat::new("Offene Forderungen", format_eur(t.receivables_cents), "nichts überfällig")
            },
            Stat::new("Offene Bestellungen", format!("{} Stk.", t.open_units), format!("{} Bestellung(en)", t.open_orders)),
            if t.queue_ends_at.is_empty() {
                Stat::new("Druck-Warteschlange", "leer", "keine Jobs geplant")
            } else {
                Stat::new("Druck-Warteschlange", format_datetime_de(&t.queue_ends_at), "voraussichtlich fertig")
            },
        ];
        widgets::stat_rows(ui, &stats, 200.0);
    }

    fn chart_ui(&self, ui: &mut egui::Ui, revenue: bool) {
        let data: Vec<(String, f64)> = self
            .months
            .iter()
            .map(|m| (month_label(&m.month), if revenue { m.revenue_cents as f64 / 100.0 } else { m.units as f64 }))
            .collect();
        let hint = "letzte 12 Monate · laufender Monat heller";
        if revenue {
            widgets::card(ui, "Umsatz je Monat", hint, |ui| bar_chart(ui, &data, &eur_short));
        } else {
            widgets::card(ui, "Absatz je Monat (Sets)", hint, |ui| bar_chart(ui, &data, &|v: f64| format!("{v:.0}")));
        }
    }

    fn reach_ui(&self, ui: &mut egui::Ui) {
        widgets::card(ui, "Materialreichweite", "Verbrauch der letzten 90 Tage · Strich = Lieferzeit", |ui| {
            if self.reach.is_empty() {
                widgets::hint(
                    ui,
                    "Noch kein Verbrauch in den letzten 90 Tagen. Verbrauch entsteht durch versendete Lieferungen (Zukauf- und \
                     Fertigteile) und abgeschlossene Druckjobs (Filament).",
                );
                return;
            }
            let c = theme::p();
            let meter_w = 170.0;
            let chip_w = 130.0;
            let name_w = (ui.available_width() - meter_w - chip_w - 2.0 * ui.spacing().item_spacing.x).max(140.0);
            for (i, r) in self.reach.iter().enumerate() {
                if i > 0 {
                    ui.add(egui::Separator::default().spacing(6.0));
                }
                {
                    let until_order = r.days_left - r.lead_time_days;
                    let (tone, label) = if r.overdue {
                        (Tone::Critical, "jetzt bestellen".to_string())
                    } else if until_order <= 14 {
                        (Tone::Warn, format!("bis {}", format_de(&r.order_by)))
                    } else {
                        (Tone::Accent, format!("bis {}", format_de(&r.order_by)))
                    };
                    let (fill, track) = match tone {
                        Tone::Critical => (c.critical, c.critical_bg),
                        Tone::Warn => (c.warn, c.warn_bg),
                        _ => (c.accent, c.accent_track),
                    };
                    ui.horizontal(|ui| {
                        let column = || egui::Layout::top_down(egui::Align::LEFT);
                        ui.allocate_ui_with_layout(egui::vec2(name_w, 36.0), column(), |ui| {
                            ui.set_width(name_w);
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(egui::Label::new(egui::RichText::new(&r.name).family(theme::semibold()).color(c.text)).truncate());
                            let stock = format!("{} {} im Lager · {} {} in 90 Tagen", r.qty, r.unit, r.consumed_90d, r.unit);
                            ui.add(egui::Label::new(egui::RichText::new(stock).size(12.0).color(c.muted)).truncate());
                        });
                        ui.allocate_ui_with_layout(egui::vec2(meter_w, 36.0), column(), |ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let marker = (r.lead_time_days > 0).then(|| r.lead_time_days as f32 / REACH_SCALE_DAYS);
                            widgets::meter(ui, meter_w, r.days_left as f32 / REACH_SCALE_DAYS, fill, track, marker)
                                .on_hover_text(format!("Skala bis {REACH_SCALE_DAYS:.0} Tage"));
                            let lead = if r.lead_time_days > 0 { format!("{} T.", r.lead_time_days) } else { "–".into() };
                            widgets::hint(ui, &format!("reicht {} Tage · Lieferzeit {lead}", r.days_left));
                        });
                        widgets::chip(ui, tone, &label);
                    });
                }
            }
        });
    }

    fn margin_ui(&self, ui: &mut egui::Ui) {
        let r = &self.rates;
        let hint = format!(
            "Maschine {}/h · Arbeit {}/h · Filament {}/kg",
            eur(r.machine_eur_per_h),
            eur(r.labor_eur_per_h),
            eur(r.filament_eur_per_g * 1000.0)
        );
        widgets::card(ui, "Marge je Produkt", &hint, |ui| {
            if self.costs.is_empty() {
                widgets::hint(ui, "Noch keine Produkte angelegt.");
                return;
            }
            let c = theme::p();
            for (n, cost) in self.costs.iter().enumerate() {
                if n > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.label(theme::strong(&cost.name, 15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::strong(eur(cost.cost_total()), 14.0));
                        widgets::form_label(ui, "Selbstkosten je Stück");
                    });
                });
                cost_bar(ui, &[(cost.purchase_eur, "Zukaufteile"), (cost.filament_eur, "Filament"), (cost.machine_eur, "Druckzeit"), (cost.labor_eur, "Arbeitszeit")]);
                legend(
                    ui,
                    &[
                        "Zukaufteile".to_string(),
                        format!("Filament ({} g)", widgets::dec(cost.filament_g, 0)),
                        format!("Druckzeit ({} h)", widgets::dec(cost.print_hours, 2)),
                        "Arbeitszeit".to_string(),
                    ],
                );
                if !cost.tiers.is_empty() {
                    egui::Grid::new(("tier_grid", cost.product_id)).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
                        widgets::table_head(ui, &["Staffel", "Preis", "Gewinn vor Arbeit", "nach Arbeit", "Marge"]);
                        for (qty, price) in &cost.tiers {
                            let before = cost.profit_before_labor(*price);
                            let after = cost.profit_after_labor(*price);
                            ui.label(format!("ab {qty} Stk."));
                            ui.label(format_eur(*price));
                            ui.label(egui::RichText::new(eur(before)).color(if before < 0.0 { c.critical_text } else { c.text }));
                            ui.label(egui::RichText::new(eur(after)).color(if after < 0.0 { c.warn_text } else { c.text }));
                            let margin = cost.margin_pct(*price).map_or("–".to_string(), |m| format!("{m:.0} %"));
                            if after < 0.0 {
                                widgets::chip(ui, Tone::Warn, &format!("{margin} · Arbeit nicht gedeckt"));
                            } else {
                                ui.label(theme::strong(margin, 14.0));
                            }
                            ui.end_row();
                        }
                    });
                }
                for w in &cost.warnings {
                    ui.colored_label(c.warn_text, w.as_str());
                }
            }
            ui.add_space(4.0);
            widgets::hint(
                ui,
                "Druckzeit und Filament je Teil stammen aus der Plattenvorlage mit den meisten Exemplaren. Versandkosten sind nicht enthalten.",
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_steps_are_round() {
        assert_eq!(nice_axis(1300.0), (1500.0, 500.0));
        assert_eq!(nice_axis(864.0), (1000.0, 250.0));
        assert_eq!(nice_axis(45.0), (60.0, 20.0));
        assert_eq!(nice_axis(0.0), (1.0, 0.25));
        assert_eq!(eur_short(1250.0), "1.250 €");
        assert_eq!(eur_short(0.0), "0 €");
    }
}
