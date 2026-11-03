use druckplaner_core::{
    catalog::Material,
    date::{format_datetime_de, format_de},
    production::{JobStatus, NeedRow, PlateTemplate, PrintJob, Schedule, Suggestions},
    Db,
};
use eframe::egui;

use crate::widgets;

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
    Reopen(i64),
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
    /// Verbrauch in Gramm, der bei „Fehldruck“ abgebucht wird
    fail_g: i64,
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
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Act::None;
        let error = ui.visuals().error_fg_color;
        let warn = ui.visuals().warn_fg_color;

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Produktion");
            let queued = self.schedule.jobs.len();
            if queued == 0 {
                ui.label("Die Warteschlange ist leer.");
            } else {
                ui.label(format!("{queued} Job(s) in der Warteschlange · voraussichtlich fertig {}", format_datetime_de(&self.schedule.ends_at)));
            }
            for f in self.schedule.filament.iter().filter(|f| f.need_g > f.stock_g) {
                ui.colored_label(
                    error,
                    format!("Filament {}: Warteschlange braucht {} g, im Lager {} g – nachbestellen", f.material_label, f.need_g, f.stock_g),
                );
            }

            // ---- Bedarf
            ui.add_space(12.0);
            ui.strong("Bedarf an fertigen Teilen");
            if self.needs.is_empty() {
                ui.weak("Kein Bedarf: keine offenen Bestellungen mit Produkten und keine Mindestbestände für Fertigteile.");
            } else {
                egui::Grid::new("needs").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for h in ["Teil", "Farbe", "Bestellt", "Mindest.", "Lager", "Geplant", "Fehlt", "Termin"] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for n in &self.needs {
                        ui.label(n.part_name.as_str());
                        ui.label(n.material_label.as_str());
                        ui.label(n.demand.to_string());
                        ui.label(n.min_target.to_string());
                        ui.label(n.stock.to_string());
                        ui.label(n.planned.to_string());
                        if n.missing() > 0 {
                            ui.colored_label(error, n.missing().to_string());
                        } else {
                            ui.label("0");
                        }
                        ui.label(if n.earliest_due.is_empty() { String::new() } else { format_de(&n.earliest_due) });
                        ui.end_row();
                    }
                });
                ui.weak("Fehlt = Bestellt + Mindestbestand − Lager − Geplant. Mindestbestände für Fertigteile stellst du im Lager beim jeweiligen Posten ein.");
            }

            // ---- Vorschläge
            ui.add_space(12.0);
            ui.strong("Vorschläge");
            if self.suggestions.items.is_empty() && self.suggestions.unmet.is_empty() {
                ui.weak("Nichts zu drucken – der Bedarf ist gedeckt oder bereits eingeplant.");
            }
            for (i, s) in self.suggestions.items.iter().enumerate() {
                ui.horizontal(|ui| {
                    let produces = s.produces.iter().map(|(n, q)| format!("{n} +{q}")).collect::<Vec<_>>().join(", ");
                    ui.label(format!(
                        "{}× {} in {} → {} ({} h {:02} min, {:.0} g)",
                        s.count,
                        s.template_name,
                        s.material_label,
                        produces,
                        s.minutes_total / 60,
                        s.minutes_total % 60,
                        s.filament_g_total
                    ));
                    if ui.button("Einplanen").clicked() {
                        act = Act::ApplySuggestion(i);
                    }
                });
            }
            for u in &self.suggestions.unmet {
                ui.colored_label(warn, u.as_str());
            }
            if self.suggestions.items.len() > 1 && ui.button("Alle Vorschläge einplanen").clicked() {
                act = Act::ApplyAll;
            }

            // ---- Warteschlange
            ui.add_space(12.0);
            ui.strong("Warteschlange");
            let printing = self.schedule.jobs.iter().any(|j| j.job.status == JobStatus::Printing);
            egui::Grid::new("queue").striped(true).spacing([14.0, 5.0]).show(ui, |ui| {
                for h in ["Platte", "Farbe", "Start", "Ende", "Status", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for j in &self.schedule.jobs {
                    ui.label(j.template_name.as_str());
                    ui.label(j.material_label.as_str());
                    ui.label(format_datetime_de(&j.start));
                    ui.label(format_datetime_de(&j.end));
                    ui.label(j.job.status.label());
                    ui.horizontal(|ui| {
                        let id = j.job.id;
                        match j.job.status {
                            JobStatus::Planned => {
                                if ui.small_button("Hoch").clicked() {
                                    act = Act::Move(id, true);
                                }
                                if ui.small_button("Runter").clicked() {
                                    act = Act::Move(id, false);
                                }
                                if ui.add_enabled(!printing, egui::Button::new("Starten").small()).clicked() {
                                    act = Act::Start(id);
                                }
                                if ui.small_button("Fertig").clicked() {
                                    act = Act::Finish(id);
                                }
                                if ui.small_button("Entfernen").clicked() {
                                    act = Act::Delete(id);
                                }
                            }
                            JobStatus::Printing => {
                                if ui.small_button("Fertig").clicked() {
                                    act = Act::Finish(id);
                                }
                                if ui.small_button("Fehldruck").on_hover_text("Bucht den Verbrauch (rechts unter der Tabelle) ab, ohne Teile").clicked() {
                                    act = Act::Fail(id);
                                }
                            }
                            _ => {}
                        }
                    });
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Verbrauch bei Fehldruck");
                ui.add(egui::DragValue::new(&mut self.fail_g).range(0..=10_000).suffix(" g"));
            });

            // ---- Job manuell anlegen
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("job_template")
                    .width(200.0)
                    .selected_text(self.templates.iter().find(|t| t.id == self.new_template).map_or("Platte", |t| t.name.as_str()))
                    .show_ui(ui, |ui| {
                        for t in &self.templates {
                            ui.selectable_value(&mut self.new_template, t.id, t.name.as_str());
                        }
                    });
                egui::ComboBox::from_id_salt("job_material")
                    .width(180.0)
                    .selected_text(self.materials.iter().find(|m| m.id == self.new_material).map_or("Farbe".to_string(), Material::label))
                    .show_ui(ui, |ui| {
                        for m in &self.materials {
                            ui.selectable_value(&mut self.new_material, m.id, m.label());
                        }
                    });
                if ui.add_enabled(self.new_template != 0 && self.new_material != 0, egui::Button::new("Job anlegen")).clicked() {
                    act = Act::AddJob;
                }
            });
            if self.templates.is_empty() {
                ui.weak("Lege zuerst unter Stammdaten › Plattenvorlagen eine Platte an (z. B. aus einer .3mf-Datei).");
            }

            // ---- Abgeschlossen
            ui.add_space(12.0);
            egui::CollapsingHeader::new(format!("Zuletzt abgeschlossen ({})", self.closed.len())).default_open(false).show(ui, |ui| {
                egui::Grid::new("closed_jobs").striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for j in &self.closed {
                        let template = self.templates.iter().find(|t| t.id == j.template_id).map_or("?", |t| t.name.as_str());
                        let material = self.materials.iter().find(|m| m.id == j.material_id).map_or(String::new(), Material::label);
                        ui.label(template);
                        ui.label(material);
                        ui.label(format_datetime_de(&j.finished_at));
                        ui.label(format!("{} · {} g", j.status.label(), j.used_g));
                        if ui.small_button("Zurücknehmen").on_hover_text("Macht die Lagerbuchungen rückgängig").clicked() {
                            act = Act::Reopen(j.id);
                        }
                        ui.end_row();
                    }
                });
            });
        });

        self.handle(act, db, status);
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
            Act::Fail(id) => widgets::report(status, db.fail_job(id, self.fail_g), "Fehldruck gebucht (Filament abgezogen)").is_some(),
            Act::Reopen(id) => widgets::report(status, db.reopen_job(id), "Job zurückgenommen, Lagerbuchungen entfernt").is_some(),
        };
        if done {
            self.reload(db, status);
        }
    }
}
