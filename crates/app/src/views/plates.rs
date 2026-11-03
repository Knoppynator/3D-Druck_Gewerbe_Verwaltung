use std::{collections::HashMap, path::PathBuf};

use druckplaner_core::{
    catalog::PrintPart,
    production::{PlateTemplate, TemplatePart},
    threemf::{parts_from_objects, read_3mf, ObjectCount, Plate3mf},
    Db,
};
use eframe::egui;

use crate::widgets::{self, Action};

/// Geladene .3mf-Datei: Platten zur Auswahl und Zuordnung Objektname → Druckteil (0 = ignorieren).
struct Import {
    file: PathBuf,
    plates: Vec<Plate3mf>,
    pick: usize,
    mapping: HashMap<String, i64>,
}

#[derive(Default)]
pub struct Plates {
    items: Vec<PlateTemplate>,
    form: PlateTemplate,
    print_parts: Vec<PrintPart>,
    kinds: Vec<String>,
    import: Option<Import>,
    /// Datei, die beim Speichern der Vorlage abgelegt wird
    pending_file: Option<PathBuf>,
}

enum Extra {
    None,
    PickFile,
    Adopt,
    CancelImport,
}

fn duration_text(seconds: i64) -> String {
    let m = (seconds + 59) / 60;
    format!("{} h {:02} min", m / 60, m % 60)
}

impl Plates {
    pub fn reload(&mut self, db: &Db, status: &mut String) {
        if let Some(v) = widgets::report(status, db.list_plate_templates(), "") {
            self.items = v;
        }
        if let Some(v) = widgets::report(status, db.list_print_parts(), "") {
            self.print_parts = v;
        }
        if let Some(v) = widgets::report(status, db.material_kinds(), "") {
            self.kinds = v;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, db: &Db, status: &mut String) {
        let mut act = Action::None;
        let mut extra = Extra::None;

        egui::SidePanel::left("plates_list").default_width(280.0).show_inside(ui, |ui| {
            if ui.button("Neue Plattenvorlage").clicked() {
                act = Action::New;
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for t in &self.items {
                    let text = format!("{}\n{} · {} g", t.name, t.duration_label(), widgets::dec(t.filament_g, 0));
                    if ui.selectable_label(self.form.id == t.id, text).clicked() {
                        act = Action::Select(t.id);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.detail_ui(ui, &mut act, &mut extra));
        });

        match extra {
            Extra::None => {}
            Extra::PickFile => self.pick_file(db, status),
            Extra::Adopt => self.adopt(db, status),
            Extra::CancelImport => self.import = None,
        }

        match act {
            Action::None => {}
            Action::New => {
                self.form = PlateTemplate::default();
                self.pending_file = None;
                self.import = None;
            }
            Action::Select(id) => {
                if let Some(t) = self.items.iter().find(|t| t.id == id) {
                    self.form = t.clone();
                    self.pending_file = None;
                    self.import = None;
                }
            }
            Action::Save => {
                if widgets::report(status, db.save_plate_template(&mut self.form), "Plattenvorlage gespeichert").is_some() {
                    if let Some(file) = self.pending_file.take() {
                        if let Some(rel) = widgets::report(status, db.attach_3mf(self.form.id, &file), "") {
                            self.form.file_3mf = rel;
                        }
                    }
                    self.reload(db, status);
                }
            }
            Action::Delete => {
                if widgets::report(status, db.delete_plate_template(self.form.id), "Plattenvorlage gelöscht").is_some() {
                    self.form = PlateTemplate::default();
                    self.reload(db, status);
                }
            }
        }
    }

    fn pick_file(&mut self, db: &Db, status: &mut String) {
        let Some(path) = rfd::FileDialog::new().set_title("Bambu-Studio-Datei wählen").add_filter("3MF", &["3mf"]).pick_file() else {
            return;
        };
        let Some(plates) = widgets::report(status, read_3mf(&path), "") else { return };
        // Vorbelegung der Zuordnung für alle Objektnamen aller Platten
        let mut all: Vec<ObjectCount> = Vec::new();
        for p in &plates {
            for c in p.object_counts() {
                if !all.iter().any(|x| x.name == c.name) {
                    all.push(c);
                }
            }
        }
        let Some(mapping) = widgets::report(status, db.object_mapping(&all, &self.print_parts), "") else { return };
        *status = format!("{} Platte(n) in der Datei gefunden", plates.len());
        self.import = Some(Import { file: path, plates, pick: 0, mapping });
    }

    /// Übernimmt die gewählte Platte der .3mf-Datei in das Formular und merkt sich die Zuordnung.
    fn adopt(&mut self, db: &Db, status: &mut String) {
        let Some(import) = self.import.take() else { return };
        let Some(plate) = import.plates.get(import.pick) else { return };
        for (name, part_id) in &import.mapping {
            if let Err(e) = db.remember_part_for_object(name, *part_id) {
                *status = format!("Fehler: {e}");
            }
        }
        let parts = parts_from_objects(&plate.object_counts(), &import.mapping);
        self.form.parts = parts.into_iter().map(|(print_part_id, qty)| TemplatePart { print_part_id, qty }).collect();
        if plate.sliced {
            self.form.print_seconds = plate.print_seconds;
            self.form.filament_g = (plate.weight_g * 10.0).round() / 10.0;
        }
        if plate.filaments.len() == 1 && self.form.material_kind.is_empty() {
            let kind = plate.filaments[0].kind.clone();
            if self.kinds.contains(&kind) {
                self.form.material_kind = kind;
            }
        }
        if self.form.name.trim().is_empty() {
            let file = import.file.file_name().and_then(|n| n.to_str()).unwrap_or("Platte");
            let stem = file.trim_end_matches(".3mf").trim_end_matches(".gcode");
            self.form.name = if plate.sliced { format!("{stem} Platte {}", plate.index) } else { stem.to_string() };
        }
        self.pending_file = Some(import.file);
        *status = if plate.sliced {
            "Werte aus der Datei übernommen".to_string()
        } else {
            "Teile übernommen – Druckzeit und Filament stehen erst nach dem Slicen in der Datei, bitte von Hand eintragen".to_string()
        };
    }

    fn detail_ui(&mut self, ui: &mut egui::Ui, act: &mut Action, extra: &mut Extra) {
        ui.heading(if self.form.id == 0 { "Neue Plattenvorlage" } else { "Plattenvorlage bearbeiten" });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui
                .button("Aus Bambu-Studio-Datei übernehmen …")
                .on_hover_text("Liest die Objekte auf der Platte; bei gesliceten Dateien auch Druckzeit und Gewicht")
                .clicked()
            {
                *extra = Extra::PickFile;
            }
            if let Some(f) = &self.pending_file {
                ui.weak(format!("Datei wird beim Speichern abgelegt: {}", f.file_name().and_then(|n| n.to_str()).unwrap_or("")));
            } else if !self.form.file_3mf.is_empty() {
                ui.weak(format!("Abgelegt: {}", self.form.file_3mf));
            }
        });
        self.import_ui(ui, extra);
        ui.add_space(8.0);

        egui::Grid::new("plate_form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            widgets::text_row(ui, "Name", &mut self.form.name);
            ui.label("Werkstoff");
            egui::ComboBox::from_id_salt("plate_kind")
                .width(200.0)
                .selected_text(if self.form.material_kind.is_empty() { "(beliebig)" } else { self.form.material_kind.as_str() })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.form.material_kind, String::new(), "(beliebig)");
                    for k in &self.kinds {
                        ui.selectable_value(&mut self.form.material_kind, k.clone(), k.as_str());
                    }
                });
            ui.end_row();
            ui.label("Druckzeit");
            ui.horizontal(|ui| {
                let mut hours = self.form.print_seconds / 3600;
                let mut minutes = (self.form.print_seconds % 3600) / 60;
                let h = ui.add(egui::DragValue::new(&mut hours).range(0..=999).suffix(" h")).changed();
                let m = ui.add(egui::DragValue::new(&mut minutes).range(0..=59).suffix(" min")).changed();
                if h || m {
                    self.form.print_seconds = hours * 3600 + minutes * 60;
                }
            });
            ui.end_row();
            ui.label("Filament");
            widgets::dec_drag(ui, &mut self.form.filament_g, 0.0..=100_000.0, 0.5, 1, " g");
            ui.end_row();
            widgets::text_row(ui, "Notiz", &mut self.form.note);
        });

        ui.add_space(10.0);
        ui.strong("Teile auf der Platte");
        if self.print_parts.is_empty() {
            ui.label("Lege zuerst unter Stammdaten › Druckteile die Teile an.");
        }
        let mut remove = None;
        for (i, p) in self.form.parts.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt(("plate_part", i))
                    .width(220.0)
                    .selected_text(self.print_parts.iter().find(|x| x.id == p.print_part_id).map_or("wählen", |x| x.name.as_str()))
                    .show_ui(ui, |ui| {
                        for x in &self.print_parts {
                            ui.selectable_value(&mut p.print_part_id, x.id, x.name.as_str());
                        }
                    });
                ui.add(egui::DragValue::new(&mut p.qty).range(1..=1000).prefix("× "));
                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.form.parts.remove(i);
        }
        if ui.button("+ Teil").clicked() {
            let first = self.print_parts.first().map_or(0, |p| p.id);
            self.form.parts.push(TemplatePart { print_part_id: first, qty: 1 });
        }

        ui.add_space(10.0);
        widgets::form_buttons(ui, self.form.id != 0, act);
    }

    /// Platten der geladenen Datei, Zuordnung der Objekte zu Druckteilen und „Übernehmen“.
    fn import_ui(&mut self, ui: &mut egui::Ui, extra: &mut Extra) {
        let Some(import) = &mut self.import else { return };
        ui.add_space(6.0);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let file = import.file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            ui.strong(format!("Datei: {file}"));
            if import.plates.len() > 1 {
                ui.label("Platte wählen:");
            }
            for (i, p) in import.plates.iter().enumerate() {
                let objects = p.object_counts().iter().map(|c| format!("{}×{}", c.name, c.count)).collect::<Vec<_>>().join(", ");
                let facts = if p.sliced {
                    format!("{} · {} g", duration_text(p.print_seconds), widgets::dec(p.weight_g, 1))
                } else {
                    "nicht gesliced".to_string()
                };
                ui.selectable_value(&mut import.pick, i, format!("Platte {} · {facts} · {objects}", p.index));
            }
            let plate = import.plates.get(import.pick);
            if plate.is_some_and(|p| !p.sliced) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "Die Datei enthält keine Slicing-Daten, deshalb fehlen Druckzeit und Filamentverbrauch. Du trägst sie unten \
                     von Hand ein – oder du slicest die Platte in Bambu Studio und exportierst die Plattendatei (.gcode.3mf).",
                );
            }

            ui.add_space(4.0);
            ui.strong("Objekt → Druckteil");
            if let Some(plate) = plate {
                for c in plate.object_counts() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} × {}", c.count, c.name));
                        let current = import.mapping.entry(c.name.clone()).or_insert(0);
                        let selected = self.print_parts.iter().find(|p| p.id == *current).map_or("(ignorieren)", |p| p.name.as_str());
                        egui::ComboBox::from_id_salt(("map3mf", &c.name)).width(220.0).selected_text(selected).show_ui(ui, |ui| {
                            ui.selectable_value(current, 0, "(ignorieren)");
                            for p in &self.print_parts {
                                ui.selectable_value(current, p.id, p.name.as_str());
                            }
                        });
                    });
                }
            }
            ui.weak("Hilfskörper wie Verbinder-Würfel auf „ignorieren“ lassen. Deine Zuordnung wird für künftige Importe gemerkt.");
            ui.horizontal(|ui| {
                if ui.button("Werte übernehmen").clicked() {
                    *extra = Extra::Adopt;
                }
                if ui.button("Abbrechen").clicked() {
                    *extra = Extra::CancelImport;
                }
            });
        });
    }
}
