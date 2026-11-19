use druckplaner_core::{
    catalog::{Material, PrintPart},
    date::{self, datetime_to_minutes, format_datetime_de, format_de, minutes_to_datetime},
    production::{JobStatus, NeedRow, PlateTemplate, PrintJob, Schedule, ScheduledJob, StartTimes, Suggestions},
    Db,
};
use eframe::egui;

use crate::{
    icons::Icon,
    theme,
    widgets::{self, Stat, Tone},
};

const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

enum Act {
    None,
    AddJob,
    ApplySuggestion(usize),
    ApplyAll,
    Move(i64, bool),
    Delete(i64),
    Start(i64),
    Finish(i64),
    Fail(i64),
    ConfirmFail,
    CancelFail,
    Reopen(i64),
}

/// Eingabe für einen Fehldruck: Verbrauch und brauchbare Teile je Druckteil der Platte.
struct FailForm {
    job_id: i64,
    title: String,
    used_g: i64,
    default_g: i64,
    /// (Druckteil-ID, Name, Stück je Platte, davon brauchbar)
    parts: Vec<(i64, String, i64, i64)>,
}

#[derive(Default)]
pub struct Production {
    needs: Vec<NeedRow>,
    suggestions: Suggestions,
    schedule: Schedule,
    closed: Vec<PrintJob>,
    templates: Vec<PlateTemplate>,
    materials: Vec<Material>,
    new_template: i64,
    new_material: i64,
    print_parts: Vec<PrintPart>,
    fail: Option<FailForm>,
    /// Zeitpunkt des Ladens (`JJJJ-MM-TT HH:MM`) für Fortschritt und „jetzt“-Linie
    now: String,
    /// Druckstart-Zeiten je Wochentag (Einstellungen)
    starts: StartTimes,
}

/// `JJJJ-MM-TT HH:MM` → `Mi 18:00`.
fn short_time(minutes: i64) -> String {
    let s = minutes_to_datetime(minutes);
    let wd = match (s[0..4].parse(), s[5..7].parse(), s[8..10].parse()) {
        (Ok(y), Ok(m), Ok(d)) => WEEKDAYS[date::weekday(y, m, d) as usize],
        _ => "",
    };
    format!("{wd} {}", &s[11..16])
}

fn minutes_label(m: i64) -> String {
    format!("{} h {:02} min", m / 60, m % 60)
}

impl Production {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.production_needs(), "") {
            self.needs = v;
        }
        if let Some(v) = widgets::report(status, db.suggest_jobs(), "") {
            self.suggestions = v;
        }
        if let Some(v) = widgets::report(status, db.schedule(), "") {
            self.schedule = v;
        }
        if let Some(v) = widgets::report(status, db.list_jobs(true), "") {
            self.closed = v;
        }
        if let Some(v) = widgets::report(status, db.list_plate_templates(), "") {
            self.templates = v;
        }
        if let Some(v) = widgets::report(status, db.list_materials(), "") {
            self.materials = v;
        }
        if let Some(v) = widgets::report(status, db.list_print_parts(), "") {
            self.print_parts = v;
        }
        if let Some(v) = widgets::report(status, db.now(), "") {
            self.now = v;
        }
        if let Some(v) = widgets::report(status, db.start_times(), "") {
            self.starts = v;
        }
    }

    /// Anzeigefarbe des Filaments
    fn color_of(&self, material_id: i64) -> egui::Color32 {
        widgets::filament_color(self.materials.iter().find(|m| m.id == material_id))
    }

    /// Druckstart-Zeiten heute, z. B. „06:30–07:30, 18:00–22:00“
    fn starts_today(&self) -> String {
        let day = datetime_to_minutes(&self.now).unwrap_or(0).div_euclid(1440);
        StartTimes::describe(&self.starts.days[StartTimes::weekday_of_day(day)])
    }

    fn template(&self, id: i64) -> Option<&PlateTemplate> {
        self.templates.iter().find(|t| t.id == id)
    }

    /// „6× Gehäuse, 2× Deckel“
    fn parts_text(&self, t: &PlateTemplate) -> String {
        t.parts
            .iter()
            .map(|p| {
                let name = self.print_parts.iter().find(|n| n.id == p.print_part_id).map_or("?", |n| n.name.as_str());
                format!("{}× {name}", p.qty)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let queued = self.schedule.jobs.len();
            let subtitle = if queued == 0 {
                format!("Die Warteschlange ist leer · Druckstart heute {}", self.starts_today())
            } else {
                format!(
                    "{queued} Job(s) in der Warteschlange · voraussichtlich fertig {} · Druckstart heute {}",
                    format_datetime_de(&self.schedule.ends_at),
                    self.starts_today()
                )
            };
            let suggestions = self.suggestions.items.len();
            widgets::page_header(ui, "Produktion", &subtitle, |ui| {
                if suggestions > 0 && widgets::primary_button(ui, "Alle Vorschläge einplanen").clicked() {
                    act = Act::ApplyAll;
                }
            });
            for f in self.schedule.filament.iter().filter(|f| f.need_g > f.stock_g) {
                let text = format!("{} – die Warteschlange braucht {} g, im Lager sind {} g.", f.material_label, f.need_g, f.stock_g);
                widgets::alert(ui, Tone::Critical, "Filament fehlt:", &text, None);
            }

            if ui.available_width() >= 900.0 {
                ui.columns(2, |cols| {
                    self.current_ui(&mut cols[0], &mut act);
                    self.stats_ui(&mut cols[1]);
                });
            } else {
                self.current_ui(ui, &mut act);
                self.stats_ui(ui);
            }
            self.queue_ui(ui, &mut act);
            if ui.available_width() >= 1300.0 {
                ui.columns(2, |cols| {
                    self.needs_ui(&mut cols[0]);
                    self.suggestions_ui(&mut cols[1], &mut act);
                });
            } else {
                self.needs_ui(ui);
                self.suggestions_ui(ui, &mut act);
            }
            self.closed_ui(ui, &mut act);
        });

        self.fail_window(ui.ctx(), &mut act);
        self.handle(act, db, status);
    }

    /// Karte „Druckt gerade“ bzw. „Drucker frei“ mit dem nächsten Job.
    fn current_ui(&self, ui: &mut egui::Ui, act: &mut Act) {
        let c = theme::p();
        let running = self.schedule.jobs.iter().find(|j| j.job.status == JobStatus::Printing);
        let shown = running.or(self.schedule.jobs.first()).map(|j| self.color_of(j.job.material_id));
        let frame = widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(j) = running.or(self.schedule.jobs.first()) else {
                ui.horizontal(|ui| {
                    widgets::chip(ui, Tone::Neutral, "Drucker frei");
                    ui.label(theme::strong("Nichts geplant", 16.0));
                });
                widgets::hint(ui, "Übernimm unten einen Vorschlag oder lege einen Job an.");
                return;
            };
            let printing = running.is_some();
            ui.horizontal(|ui| {
                if printing {
                    widgets::chip(ui, Tone::Good, "Druckt");
                } else {
                    widgets::chip(ui, Tone::Neutral, "Drucker frei – als Nächstes");
                }
                ui.label(theme::strong(&j.template_name, 16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&j.material_label).color(c.text_soft));
                    widgets::swatch(ui, self.color_of(j.job.material_id), 14.0);
                });
            });
            let (start, end, now) = (datetime_to_minutes(&j.start), datetime_to_minutes(&j.end), datetime_to_minutes(&self.now));
            if let (Some(start), Some(end), Some(now)) = (start, end, now) {
                let total = (end - start).max(1);
                let done = if printing { (now - start).clamp(0, total) } else { 0 };
                let fraction = done as f32 / total as f32;
                widgets::meter(ui, ui.available_width(), fraction, c.accent, c.accent_track, None);
                ui.horizontal(|ui| {
                    widgets::hint(ui, &format!("Start {}", short_time(start)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::hint(ui, &format!("fertig ca. {}", short_time(end)));
                        if printing {
                            ui.add_space(12.0);
                            ui.label(theme::strong(format!("{:.0} %", fraction * 100.0), 13.0));
                            widgets::hint(ui, &format!("noch ca. {}", minutes_label((total - done).max(0))));
                        }
                    });
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if let Some(t) = self.template(j.job.template_id) {
                    widgets::hint(ui, &format!("Auf der Platte: {} · {} g · {}", self.parts_text(t), widgets::dec(t.filament_g, 0), t.duration_label()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if printing {
                        let done = egui::Button::new(egui::RichText::new("Fertig").family(theme::semibold()).color(egui::Color32::WHITE))
                            .fill(c.good_text);
                        if ui.add(done).on_hover_text("Teile ins Lager, Filament laut Platte abbuchen").clicked() {
                            *act = Act::Finish(j.job.id);
                        }
                        if widgets::danger_button(ui, "Fehldruck …").clicked() {
                            *act = Act::Fail(j.job.id);
                        }
                    } else if widgets::primary_button(ui, "Starten").clicked() {
                        *act = Act::Start(j.job.id);
                    }
                });
            });
        });
        if let Some(color) = shown {
            widgets::color_stripe(ui, frame.response.rect, 12, color);
        }
        ui.add_space(4.0);
    }

    fn stats_ui(&self, ui: &mut egui::Ui) {
        let jobs = &self.schedule.jobs;
        let printing = jobs.iter().filter(|j| j.job.status == JobStatus::Printing).count();
        let minutes: i64 = jobs.iter().filter_map(|j| self.template(j.job.template_id)).map(PlateTemplate::minutes).sum();
        let filament: i64 = self.schedule.filament.iter().map(|f| f.need_g).sum();
        let missing = self.schedule.filament.iter().filter(|f| f.need_g > f.stock_g).count();
        let stats = [
            Stat::new("Jobs", jobs.len().to_string(), format!("{printing} läuft, {} geplant", jobs.len() - printing)),
            Stat::new("Druckzeit", format!("{} h", minutes / 60), "alle Jobs zusammen"),
            if missing > 0 {
                Stat::new("Filament", format!("{filament} g"), format!("{missing} Farbe(n) reichen nicht")).tone(Tone::Critical)
            } else {
                Stat::new("Filament", format!("{filament} g"), "im Lager vorhanden")
            },
        ];
        widgets::stat_rows(ui, &stats, 150.0);
    }

    /// Wartet der Job auf das Zeitfenster für den Plattenwechsel? Dann die Uhrzeit, bis zu der er wartet.
    fn waits_until(&self, i: usize, j: &ScheduledJob) -> Option<i64> {
        if j.job.status != JobStatus::Planned {
            return None;
        }
        let start = datetime_to_minutes(&j.start)?;
        let ready = match i {
            0 => datetime_to_minutes(&self.now)?,
            _ => datetime_to_minutes(&self.schedule.jobs[i - 1].end)?,
        };
        (start > ready + 1).then_some(start)
    }

    fn queue_ui(&mut self, ui: &mut egui::Ui, act: &mut Act) {
        let printing = self.schedule.jobs.iter().any(|j| j.job.status == JobStatus::Printing);
        widgets::card(ui, "Warteschlange", "", |ui| {
            if self.schedule.jobs.is_empty() {
                widgets::hint(ui, "Keine Jobs geplant.");
            } else {
                self.timeline(ui);
                ui.add_space(6.0);
                egui::Grid::new("queue").striped(true).spacing([14.0, 6.0]).min_col_width(20.0).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    widgets::table_head(ui, &["#", "Platte", "Farbe", "Start → Ende", "Status", ""]);
                    let c = theme::p();
                    for (i, j) in self.schedule.jobs.iter().enumerate() {
                        let id = j.job.id;
                        ui.label(egui::RichText::new((i + 1).to_string()).family(theme::semibold()).color(c.muted));
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(&j.template_name);
                            if let Some(t) = self.template(j.job.template_id) {
                                widgets::hint(ui, &format!("{} · {} g", self.parts_text(t), widgets::dec(t.filament_g, 0)));
                            }
                        });
                        widgets::swatch_label(ui, self.color_of(j.job.material_id), &j.material_label);
                        let (s, e) = (datetime_to_minutes(&j.start), datetime_to_minutes(&j.end));
                        match (s, e) {
                            (Some(s), Some(e)) => ui.label(format!("{} → {}", short_time(s), short_time(e))),
                            _ => ui.label(format!("{} → {}", format_datetime_de(&j.start), format_datetime_de(&j.end))),
                        };
                        match (j.job.status, self.waits_until(i, j)) {
                            (JobStatus::Printing, _) => widgets::chip(ui, Tone::Accent, "läuft"),
                            (_, Some(until)) => {
                                // am selben Tag nur die Uhrzeit, sonst mit Wochentag („wartet bis Sa 09:00“)
                                let ready = if i == 0 { datetime_to_minutes(&self.now) } else { datetime_to_minutes(&self.schedule.jobs[i - 1].end) };
                                let same_day = ready.is_some_and(|r| r.div_euclid(1440) == until.div_euclid(1440));
                                let when = if same_day { minutes_to_datetime(until)[11..16].to_string() } else { short_time(until) };
                                widgets::chip(ui, Tone::Warn, &format!("wartet bis {when}"))
                            }
                            _ => widgets::chip(ui, Tone::Neutral, "geplant"),
                        };
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            match j.job.status {
                                JobStatus::Planned => {
                                    if widgets::icon_button(ui, Icon::ChevronUp, "Nach oben", None).clicked() {
                                        *act = Act::Move(id, true);
                                    }
                                    if widgets::icon_button(ui, Icon::ChevronDown, "Nach unten", None).clicked() {
                                        *act = Act::Move(id, false);
                                    }
                                    let start = ui.add_enabled_ui(!printing, |ui| widgets::icon_button(ui, Icon::Play, "Druck starten", Some(c.accent_text)));
                                    if start.inner.clicked() {
                                        *act = Act::Start(id);
                                    }
                                    if widgets::icon_button(ui, Icon::Check, "Fertig – ohne Start abschließen", Some(c.good_text)).clicked() {
                                        *act = Act::Finish(id);
                                    }
                                    if widgets::icon_button(ui, Icon::Trash, "Aus der Warteschlange entfernen", Some(c.critical_text)).clicked() {
                                        *act = Act::Delete(id);
                                    }
                                }
                                JobStatus::Printing => {
                                    if widgets::icon_button(ui, Icon::Check, "Fertig", Some(c.good_text)).clicked() {
                                        *act = Act::Finish(id);
                                    }
                                    if widgets::danger_button(ui, "Fehldruck …").clicked() {
                                        *act = Act::Fail(id);
                                    }
                                }
                                _ => {}
                            }
                        });
                        ui.end_row();
                    }
                });
            }

            // Job von Hand anlegen
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                widgets::form_label(ui, "Job anlegen");
                egui::ComboBox::from_id_salt("job_template")
                    .width(200.0)
                    .selected_text(self.templates.iter().find(|t| t.id == self.new_template).map_or("Platte wählen", |t| t.name.as_str()))
                    .show_ui(ui, |ui| {
                        for t in &self.templates {
                            ui.selectable_value(&mut self.new_template, t.id, t.name.as_str());
                        }
                    });
                egui::ComboBox::from_id_salt("job_material")
                    .width(180.0)
                    .selected_text(self.materials.iter().find(|m| m.id == self.new_material).map_or("Farbe wählen".to_string(), Material::label))
                    .show_ui(ui, |ui| {
                        for m in &self.materials {
                            ui.selectable_value(&mut self.new_material, m.id, m.label());
                        }
                    });
                let ready = self.new_template != 0 && self.new_material != 0;
                if ui.add_enabled_ui(ready, |ui| widgets::soft_button(ui, "Anlegen")).inner.clicked() {
                    *act = Act::AddJob;
                }
            });
            if self.templates.is_empty() {
                widgets::hint(ui, "Lege zuerst unter Stammdaten › Plattenvorlagen eine Platte an (z. B. aus einer .3mf-Datei).");
            }
        });
    }

    /// Zeitstrahl der Warteschlange: Jobs als Balken, Zeiten ohne Plattenwechsel schraffiert, rote Linie = jetzt.
    fn timeline(&self, ui: &mut egui::Ui) {
        let c = theme::p();
        let spans: Vec<(i64, i64, bool, egui::Color32)> = self
            .schedule
            .jobs
            .iter()
            .filter_map(|j| {
                Some((datetime_to_minutes(&j.start)?, datetime_to_minutes(&j.end)?, j.job.status == JobStatus::Printing, self.color_of(j.job.material_id)))
            })
            .collect();
        let now = datetime_to_minutes(&self.now);
        let (Some(first), Some(last)) = (spans.iter().map(|s| s.0).min(), spans.iter().map(|s| s.1).max()) else { return };
        let from = now.map_or(first, |n| n.min(first));
        let to = last.max(from + 60);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 66.0), egui::Sense::hover());
        let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 44.0));
        let x_of = |m: i64| bar.left() + (m - from) as f32 / (to - from) as f32 * bar.width();
        let painter = ui.painter_at(rect);
        painter.rect_filled(bar, 8.0, c.faint);

        // Zeiten ohne Druckstart schraffieren: je Tag die Lücken zwischen den Startfenstern
        let mut day = from.div_euclid(1440) - 1;
        while day * 1440 < to {
            let mut off_from = day * 1440;
            let windows = &self.starts.days[StartTimes::weekday_of_day(day)];
            let gaps = windows.iter().map(|&(a, b)| (day * 1440 + a, day * 1440 + b)).chain(std::iter::once(((day + 1) * 1440, (day + 1) * 1440)));
            for (open, close) in gaps {
                let (a, b) = (x_of(off_from).max(bar.left()), x_of(open).min(bar.right()));
                if b > a {
                    let hatch = painter.with_clip_rect(egui::Rect::from_x_y_ranges(a..=b, bar.y_range()));
                    let mut x = a - bar.height();
                    while x < b {
                        hatch.line_segment([egui::pos2(x, bar.bottom()), egui::pos2(x + bar.height(), bar.top())], egui::Stroke::new(1.0_f32, c.line));
                        x += 7.0;
                    }
                }
                off_from = close.max(off_from);
            }
            day += 1;
        }

        let font = egui::FontId::new(12.0, theme::semibold());
        for (i, (s, e, running, fill)) in spans.iter().enumerate() {
            let r = egui::Rect::from_min_max(egui::pos2(x_of(*s) + 1.0, bar.top() + 8.0), egui::pos2(x_of(*e) - 1.0, bar.bottom() - 8.0));
            // Balken in Filamentfarbe; der laufende Druck bekommt einen kräftigen Rahmen in Akzentfarbe
            let stroke = if *running { egui::Stroke::new(2.5_f32, c.accent) } else { widgets::swatch_stroke() };
            painter.rect(r, 4.0, *fill, stroke, egui::StrokeKind::Outside);
            if r.width() > 18.0 {
                painter.text(egui::pos2(r.left() + 7.0, r.center().y), egui::Align2::LEFT_CENTER, (i + 1).to_string(), font.clone(), widgets::ink_on(*fill));
            }
        }
        if let Some(n) = now.filter(|n| (from..=to).contains(n)) {
            let x = x_of(n);
            painter.line_segment([egui::pos2(x, bar.top()), egui::pos2(x, bar.bottom())], egui::Stroke::new(2.0_f32, c.critical));
        }

        // Achse: Raster von 3, 6, 12 oder 24 Stunden, je nach Länge
        let span_h = (to - from) as f32 / 60.0;
        let step = [180, 360, 720, 1440].into_iter().find(|s| span_h * 60.0 / *s as f32 <= 8.0).unwrap_or(2880);
        let mut tick = from - from.rem_euclid(step) + step;
        while tick < to {
            let x = x_of(tick);
            painter.line_segment([egui::pos2(x, bar.bottom()), egui::pos2(x, bar.bottom() + 4.0)], egui::Stroke::new(1.0_f32, c.field_line));
            painter.text(egui::pos2(x, bar.bottom() + 6.0), egui::Align2::CENTER_TOP, short_time(tick), egui::FontId::proportional(11.0), c.muted);
            tick += step;
        }
        if let Some(pos) = response.hover_pos() {
            let m = from + ((pos.x - bar.left()) / bar.width() * (to - from) as f32) as i64;
            let hit = self.schedule.jobs.iter().enumerate().find(|(_, j)| {
                matches!((datetime_to_minutes(&j.start), datetime_to_minutes(&j.end)), (Some(s), Some(e)) if (s..e).contains(&m))
            });
            if let Some((i, j)) = hit {
                response.on_hover_text(format!("{} · {} · {}\n{} → {}", i + 1, j.template_name, j.material_label, format_datetime_de(&j.start), format_datetime_de(&j.end)));
            }
        }
        ui.horizontal(|ui| {
            widgets::hint(ui, "Balkenfarbe = Filament");
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_stroke(r, 3.0, egui::Stroke::new(2.0_f32, c.accent), egui::StrokeKind::Inside);
            widgets::hint(ui, "läuft");
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().rect_stroke(r, 3.0, egui::Stroke::new(1.0_f32, c.field_line), egui::StrokeKind::Inside);
            widgets::hint(ui, "schraffiert: kein Druckstart (Einstellungen › Druckstart-Zeiten)");
            ui.add_space(6.0);
            let (r, _) = ui.allocate_exact_size(egui::vec2(2.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, c.critical);
            widgets::hint(ui, "jetzt");
        });
    }

    fn needs_ui(&self, ui: &mut egui::Ui) {
        widgets::card(ui, "Bedarf an fertigen Teilen", "Fehlt = Bestellt + Mindestbestand − Lager − Geplant", |ui| {
            if self.needs.is_empty() {
                widgets::hint(ui, "Kein Bedarf: keine offenen Bestellungen mit Produkten und keine Mindestbestände für Fertigteile.");
                return;
            }
            egui::ScrollArea::horizontal().id_salt("needs_scroll").show(ui, |ui| {
                egui::Grid::new("needs").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    widgets::table_head(ui, &["Teil", "Farbe", "Bestellt", "Min.", "Lager", "Geplant", "Fehlt", "Termin"]);
                    for n in &self.needs {
                        ui.label(theme::strong(&n.part_name, 14.0));
                        widgets::swatch_label(ui, self.color_of(n.material_id), &n.material_label);
                        ui.label(n.demand.to_string());
                        ui.label(n.min_target.to_string());
                        ui.label(n.stock.to_string());
                        ui.label(n.planned.to_string());
                        if n.missing() > 0 {
                            widgets::chip(ui, Tone::Critical, &n.missing().to_string());
                        } else {
                            widgets::chip(ui, Tone::Good, "gedeckt");
                        }
                        ui.label(if n.earliest_due.is_empty() { String::new() } else { format_de(&n.earliest_due) });
                        ui.end_row();
                    }
                });
            });
            widgets::hint(ui, "Mindestbestände für Fertigteile stellst du im Lager beim jeweiligen Posten ein.");
        });
    }

    fn suggestions_ui(&self, ui: &mut egui::Ui, act: &mut Act) {
        widgets::card(ui, "Vorschläge", "deckt den Bedarf links", |ui| {
            if self.suggestions.items.is_empty() && self.suggestions.unmet.is_empty() {
                widgets::hint(ui, "Nichts zu drucken – der Bedarf ist gedeckt oder bereits eingeplant.");
            }
            let c = theme::p();
            for (i, s) in self.suggestions.items.iter().enumerate() {
                let frame = egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0_f32, c.line))
                    .corner_radius(10)
                    .inner_margin(egui::Margin { left: 18, right: 12, top: 10, bottom: 10 })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(theme::strong(format!("{}× {} · {}", s.count, s.template_name, s.material_label), 14.0));
                                let produces = s.produces.iter().map(|(n, q)| format!("{n} +{q}")).collect::<Vec<_>>().join(", ");
                                ui.label(egui::RichText::new(produces).size(12.0).color(c.text_soft));
                                widgets::hint(ui, &format!("{} · {} g", minutes_label(s.minutes_total), widgets::dec(s.filament_g_total, 0)));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::soft_button(ui, "Einplanen").clicked() {
                                    *act = Act::ApplySuggestion(i);
                                }
                            });
                        });
                    });
                widgets::color_stripe(ui, frame.response.rect, 10, self.color_of(s.material_id));
                ui.add_space(2.0);
            }
            for u in &self.suggestions.unmet {
                ui.colored_label(c.warn_text, u.as_str());
            }
        });
    }

    fn closed_ui(&self, ui: &mut egui::Ui, act: &mut Act) {
        widgets::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = egui::RichText::new(format!("Zuletzt abgeschlossen ({})", self.closed.len())).family(theme::semibold()).size(15.0);
            egui::CollapsingHeader::new(title).id_salt("closed_jobs_header").default_open(false).show(ui, |ui| {
                egui::Grid::new("closed_jobs").striped(true).spacing([14.0, 6.0]).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    for j in &self.closed {
                        let template = self.template(j.template_id).map_or("?", |t| t.name.as_str());
                        let material = self.materials.iter().find(|m| m.id == j.material_id).map_or(String::new(), Material::label);
                        ui.label(theme::strong(template, 14.0));
                        widgets::swatch_label(ui, self.color_of(j.material_id), &material);
                        ui.label(format_datetime_de(&j.finished_at));
                        let tone = if j.status == JobStatus::Failed { Tone::Serious } else { Tone::Good };
                        widgets::chip(ui, tone, j.status.label());
                        let usable = if j.status == JobStatus::Failed && j.parts_booked > 0 { format!(" · {} Teile brauchbar", j.parts_booked) } else { String::new() };
                        ui.label(format!("{} g{usable}", j.used_g));
                        if ui.small_button("Zurücknehmen").on_hover_text("Macht die Lagerbuchungen rückgängig").clicked() {
                            *act = Act::Reopen(j.id);
                        }
                        ui.end_row();
                    }
                });
            });
        });
    }

    fn fail_window(&mut self, ctx: &egui::Context, act: &mut Act) {
        let Some(f) = self.fail.as_mut() else { return };
        egui::Window::new("Fehldruck buchen")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(theme::strong(f.title.as_str(), 15.0));
                ui.add_space(6.0);
                widgets::sub_heading(ui, "Brauchbare Teile (kommen ins Lager)");
                for (_, name, planned, usable) in &mut f.parts {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(usable).range(0..=*planned).suffix(format!(" von {planned}")));
                        ui.label(name.as_str());
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    widgets::form_label(ui, "Verbrauchtes Filament");
                    ui.add(egui::DragValue::new(&mut f.used_g).range(0..=10_000).suffix(" g"));
                    if f.used_g != f.default_g && ui.small_button("Standard").clicked() {
                        f.used_g = f.default_g;
                    }
                });
                widgets::hint(ui, &format!("Vorgabe: Standardmenge der Platte ({} g) – bei Bedarf anpassen.", f.default_g));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if widgets::primary_button(ui, "Fehldruck buchen").clicked() {
                        *act = Act::ConfirmFail;
                    }
                    if ui.button("Abbrechen").clicked() {
                        *act = Act::CancelFail;
                    }
                });
            });
    }

    fn handle(&mut self, act: Act, db: &Db, status: &mut String) {
        let done = match act {
            Act::None => return,
            Act::AddJob => widgets::report(status, db.add_job(self.new_template, self.new_material), "Job angelegt").is_some(),
            Act::ApplySuggestion(i) => match self.suggestions.items.get(i).cloned() {
                Some(s) => widgets::report(status, db.apply_suggestion(&s), "Jobs eingeplant").is_some(),
                None => false,
            },
            Act::ApplyAll => {
                let items = self.suggestions.items.clone();
                let mut ok = true;
                for s in &items {
                    ok &= widgets::report(status, db.apply_suggestion(s), "Jobs eingeplant").is_some();
                }
                ok
            }
            Act::Move(id, up) => widgets::report(status, db.move_job(id, up), "").is_some(),
            Act::Delete(id) => widgets::report(status, db.delete_job(id), "Job entfernt").is_some(),
            Act::Start(id) => widgets::report(status, db.start_job(id), "Druck gestartet").is_some(),
            Act::Finish(id) => widgets::report(status, db.finish_job(id), "Job abgeschlossen – Teile und Filament gebucht").is_some(),
            Act::Fail(id) => {
                self.open_fail(db, status, id);
                false
            }
            Act::CancelFail => {
                self.fail = None;
                false
            }
            Act::ConfirmFail => match self.fail.take() {
                Some(f) => {
                    let usable: Vec<(i64, i64)> = f.parts.iter().map(|p| (p.0, p.3)).collect();
                    let ok = widgets::report(status, db.fail_job(f.job_id, f.used_g, &usable), "Fehldruck gebucht").is_some();
                    if !ok {
                        self.fail = Some(f);
                    }
                    ok
                }
                None => false,
            },
            Act::Reopen(id) => widgets::report(status, db.reopen_job(id), "Job zurückgenommen, Lagerbuchungen entfernt").is_some(),
        };
        if done {
            self.reload(db, status);
        }
    }

    /// Öffnet die Fehldruck-Eingabe mit den Standardwerten der Platte (Verbrauch laut Vorlage, nichts brauchbar).
    fn open_fail(&mut self, db: &Db, status: &mut String, id: i64) {
        let Some(job) = self.schedule.jobs.iter().find(|j| j.job.id == id) else { return };
        let Some(template) = self.templates.iter().find(|t| t.id == job.job.template_id) else { return };
        let Some(default_g) = widgets::report(status, db.default_job_usage_g(id), "") else { return };
        let parts = template
            .parts
            .iter()
            .map(|p| {
                let name = self.print_parts.iter().find(|n| n.id == p.print_part_id).map_or("?", |n| n.name.as_str());
                (p.print_part_id, name.to_string(), p.qty, 0)
            })
            .collect();
        self.fail = Some(FailForm {
            job_id: id,
            title: format!("{} · {}", job.template_name, job.material_label),
            used_g: default_g,
            default_g,
            parts,
        });
    }
}
