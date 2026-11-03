//! Kleine, wiederverwendbare Formular-Bausteine.

use druckplaner_core::{
    date::{format_de, parse_de},
    money::{cents_to_eur, eur_to_cents},
};
use eframe::egui;

/// Was der Benutzer in einer Liste-Detail-Ansicht ausgelöst hat.
pub enum Action {
    None,
    New,
    Select(i64),
    Save,
    Delete,
}

/// Schreibt das Ergebnis einer Operation in die Statuszeile und gibt bei Erfolg den Wert zurück.
pub fn report<T>(status: &mut String, result: druckplaner_core::Result<T>, ok: &str) -> Option<T> {
    match result {
        Ok(v) => {
            if !ok.is_empty() {
                *status = ok.to_string();
            }
            Some(v)
        }
        Err(e) => {
            *status = format!("Fehler: {e}");
            None
        }
    }
}

pub fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(320.0));
    ui.end_row();
}

pub fn int_row(ui: &mut egui::Ui, label: &str, value: &mut i64, suffix: &str) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=1_000_000).suffix(suffix));
    ui.end_row();
}

/// Euro-Eingabe für einen Cent-Betrag.
pub fn eur_edit(ui: &mut egui::Ui, cents: &mut i64) {
    let mut eur = cents_to_eur(*cents);
    let drag = egui::DragValue::new(&mut eur).speed(0.05).fixed_decimals(2).suffix(" €");
    if ui.add(drag).changed() {
        *cents = eur_to_cents(eur);
    }
}

pub fn eur_row(ui: &mut egui::Ui, label: &str, cents: &mut i64) {
    ui.label(label);
    eur_edit(ui, cents);
    ui.end_row();
}

/// Datumseingabe im Format TT.MM.JJJJ für ein ISO-Datum (leer = nicht gesetzt).
/// Ungültiger Text wird rot angezeigt und ändert das Datum nicht.
pub fn date_edit(ui: &mut egui::Ui, salt: impl std::hash::Hash, iso: &mut String) {
    let id = ui.id().with(salt);
    let (mut seen, mut text) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .unwrap_or_else(|| (iso.clone(), format_de(iso)));
    if seen != *iso {
        seen = iso.clone();
        text = format_de(iso);
    }
    let valid = text.trim().is_empty() || parse_de(&text).is_some();
    let mut edit = egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text("TT.MM.JJJJ");
    if !valid {
        edit = edit.text_color(ui.visuals().error_fg_color);
    }
    if ui.add(edit).changed() {
        if text.trim().is_empty() {
            iso.clear();
            seen.clear();
        } else if let Some(parsed) = parse_de(&text) {
            *iso = parsed.clone();
            seen = parsed;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, (seen, text)));
}

pub fn date_row(ui: &mut egui::Ui, label: &str, iso: &mut String) {
    ui.label(label);
    date_edit(ui, label, iso);
    ui.end_row();
}

/// Öffnet eine Datei mit dem Standardprogramm (Windows-Explorer).
pub fn open_file(path: &std::path::Path, status: &mut String) {
    if !path.exists() {
        *status = format!("Fehler: Nicht gefunden: {}", path.display());
        return;
    }
    if let Err(e) = std::process::Command::new("explorer").arg(path).spawn() {
        *status = format!("Fehler: Datei konnte nicht geöffnet werden: {e}");
    }
}

/// "Speichern" / "Löschen" unter einem Formular.
pub fn form_buttons(ui: &mut egui::Ui, can_delete: bool, act: &mut Action) {
    ui.horizontal(|ui| {
        if ui.button("Speichern").clicked() {
            *act = Action::Save;
        }
        if can_delete && ui.button("Löschen").clicked() {
            *act = Action::Delete;
        }
    });
}
