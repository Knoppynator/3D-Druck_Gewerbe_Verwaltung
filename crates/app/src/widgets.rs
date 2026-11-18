//! Kleine, wiederverwendbare Formular-Bausteine.

use druckplaner_core::{
    date::{self, format_de, parse_de},
    money::{cents_to_eur, eur_to_cents},
};
use eframe::egui::{self, Color32};

use crate::{icons, theme};

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

/// Zahl in deutscher Schreibweise mit Dezimalkomma, z. B. `dec(1.5, 1)` → `1,5`.
pub fn dec(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}

/// Eingabefeld für eine Dezimalzahl: Anzeige und Eingabe mit Komma (ein Punkt wird ebenfalls verstanden).
pub fn dec_drag(
    ui: &mut egui::Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    decimals: usize,
    suffix: &str,
) -> egui::Response {
    let drag = egui::DragValue::new(value)
        .range(range)
        .speed(speed)
        .custom_formatter(move |v, _| dec(v, decimals))
        .custom_parser(|text| text.trim().replace(',', ".").parse().ok())
        .suffix(suffix);
    ui.add(drag)
}

/// Euro-Eingabe für einen Cent-Betrag.
pub fn eur_edit(ui: &mut egui::Ui, cents: &mut i64) {
    let mut eur = cents_to_eur(*cents);
    if dec_drag(ui, &mut eur, f64::NEG_INFINITY..=f64::INFINITY, 0.05, 2, " €").changed() {
        *cents = eur_to_cents(eur);
    }
}

pub fn eur_row(ui: &mut egui::Ui, label: &str, cents: &mut i64) {
    ui.label(label);
    eur_edit(ui, cents);
    ui.end_row();
}

const MONTH_NAMES: [&str; 12] =
    ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"];
const WEEKDAY_NAMES: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

/// Schlüssel, unter dem die Hauptschleife das heutige Datum (ISO) für den Kalender ablegt.
pub const TODAY_KEY: &str = "today_iso";

/// Jahr und Monat eines ISO-Datums.
fn year_month(iso: &str) -> Option<(i32, u32)> {
    if !date::is_valid_iso(iso) {
        return None;
    }
    Some((iso[0..4].parse().ok()?, iso[5..7].parse().ok()?))
}

/// Inhalt des Kalender-Menüs; gibt `true` zurück, wenn ein Tag gewählt oder das Datum geleert wurde.
fn calendar_menu(ui: &mut egui::Ui, id: egui::Id, iso: &mut String) -> bool {
    let today: String = ui.ctx().data(|d| d.get_temp(egui::Id::new(TODAY_KEY))).unwrap_or_default();
    let (mut year, mut month) = ui
        .data_mut(|d| d.get_temp::<(i32, u32)>(id))
        .or_else(|| year_month(iso))
        .or_else(|| year_month(&today))
        .unwrap_or((2026, 1));

    ui.horizontal(|ui| {
        if ui.small_button("«").on_hover_text("Vorjahr").clicked() {
            year -= 1;
        }
        if ui.small_button("‹").on_hover_text("Vormonat").clicked() {
            (year, month) = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
        }
        ui.add_sized([120.0, 18.0], egui::Label::new(egui::RichText::new(format!("{} {year}", MONTH_NAMES[month as usize - 1])).strong()));
        if ui.small_button("›").on_hover_text("Folgemonat").clicked() {
            (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        }
        if ui.small_button("»").on_hover_text("Folgejahr").clicked() {
            year += 1;
        }
    });
    ui.data_mut(|d| d.insert_temp(id, (year, month)));

    let mut picked = false;
    egui::Grid::new(id.with("grid")).spacing([4.0, 4.0]).min_col_width(26.0).show(ui, |ui| {
        for name in WEEKDAY_NAMES {
            ui.vertical_centered(|ui| ui.weak(name));
        }
        ui.end_row();
        let offset = date::weekday(year, month, 1);
        for _ in 0..offset {
            ui.label("");
        }
        for day in 1..=date::days_in_month(year, month) {
            let this = format!("{year:04}-{month:02}-{day:02}");
            let mut button = egui::Button::new(day.to_string()).selected(*iso == this).min_size(egui::vec2(26.0, 20.0));
            if this == today {
                button = button.stroke(egui::Stroke::new(1.0_f32, ui.visuals().selection.bg_fill));
            }
            if ui.add(button).clicked() {
                *iso = this;
                picked = true;
            }
            if (offset + day).is_multiple_of(7) {
                ui.end_row();
            }
        }
    });
    ui.separator();
    ui.horizontal(|ui| {
        if !today.is_empty() && ui.small_button("Heute").clicked() {
            *iso = today.clone();
            picked = true;
        }
        if ui.small_button("Leeren").clicked() {
            iso.clear();
            picked = true;
        }
    });
    if picked {
        ui.close();
    }
    picked
}

/// Datumseingabe im Format TT.MM.JJJJ für ein ISO-Datum (leer = nicht gesetzt) mit Mini-Kalender zur Auswahl.
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

    let cal_id = id.with("calendar");
    let menu = ui.menu_button("📆", |ui| calendar_menu(ui, cal_id, iso));
    if menu.inner.is_none() {
        // geschlossen: beim nächsten Öffnen wieder beim gewählten Datum beginnen
        ui.data_mut(|d| d.remove::<(i32, u32)>(cal_id));
    }
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

// ------------------------------------------------------------------ Gestaltungs-Bausteine

/// Weiße Karte mit feiner Linie – der Rahmen für jeden Bereich einer Seite.
pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::CARD)
        .stroke(egui::Stroke::new(1.0_f32, theme::LINE))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(18, 14))
}

/// Seitenkopf: Titel, eine Zeile Kontext und rechts die Hauptaktionen.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(theme::strong(title, 24.0));
            if !subtitle.is_empty() {
                ui.label(egui::RichText::new(subtitle).color(theme::TEXT_SOFT));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
    ui.add_space(6.0);
}

/// Bedeutung einer Status-Anzeige; Farbe nie allein, immer mit Text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Good,
    Warn,
    Serious,
    Critical,
}

impl Tone {
    /// (Fläche, Text, Punkt)
    fn colors(self) -> (Color32, Color32, Color32) {
        match self {
            Tone::Neutral => (Color32::from_rgb(0xf1, 0xf0, 0xeb), theme::NAV_TEXT, Color32::from_rgb(0x9a, 0x99, 0x8f)),
            Tone::Accent => (theme::ACCENT_SOFT, theme::ACCENT_DARK, theme::ACCENT),
            Tone::Good => (theme::GOOD_BG, theme::GOOD_TEXT, theme::GOOD),
            Tone::Warn => (theme::WARN_BG, theme::WARN_TEXT, theme::WARN),
            Tone::Serious => (theme::SERIOUS_BG, theme::SERIOUS_TEXT, theme::SERIOUS),
            Tone::Critical => (theme::CRITICAL_BG, theme::CRITICAL_TEXT, theme::CRITICAL),
        }
    }
}

/// Status-Chip: Punkt und Text auf heller Fläche.
pub fn chip(ui: &mut egui::Ui, tone: Tone, text: &str) -> egui::Response {
    let (bg, fg, dot) = tone.colors();
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::new(12.0, theme::semibold()), fg);
    let size = egui::vec2(galley.size().x + 28.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 11.0, bg);
    painter.circle_filled(egui::pos2(rect.left() + 11.0, rect.center().y), 3.5, dot);
    painter.galley(egui::pos2(rect.left() + 19.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    response
}

/// Hinweisleiste mit Symbol; gibt `true` zurück, wenn der Link (falls angegeben) geklickt wurde.
pub fn alert(ui: &mut egui::Ui, tone: Tone, title: &str, text: &str, link: Option<&str>) -> bool {
    let (bg, fg, line, icon, icon_color) = match tone {
        Tone::Critical => (theme::CRITICAL_BG, Color32::from_rgb(0x7d, 0x1d, 0x1d), theme::CRITICAL_LINE, icons::Icon::AlertCircle, theme::CRITICAL),
        Tone::Warn | Tone::Serious => (theme::WARN_BG, theme::WARN_TEXT, theme::WARN_LINE, icons::Icon::Alert, Color32::from_rgb(0xb0, 0x78, 0x00)),
        Tone::Good => (theme::GOOD_BG, theme::GOOD_TEXT, theme::GOOD_BG, icons::Icon::Check, theme::GOOD),
        Tone::Neutral | Tone::Accent => (Color32::from_rgb(0xee, 0xf4, 0xfc), Color32::from_rgb(0x12, 0x3f, 0x74), Color32::from_rgb(0xb7, 0xd3, 0xf6), icons::Icon::Info, theme::ACCENT),
    };
    let mut clicked = false;
    egui::Frame::new()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0_f32, line))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                icons::show(ui, icon, 18.0, icon_color);
                if !title.is_empty() {
                    ui.label(egui::RichText::new(title).family(theme::semibold()).color(fg));
                }
                ui.label(egui::RichText::new(text).color(fg));
                if let Some(link) = link {
                    clicked = ui.link(egui::RichText::new(link).family(theme::semibold()).color(fg)).clicked();
                }
            });
        });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_use_a_comma() {
        assert_eq!(dec(1.5, 1), "1,5");
        assert_eq!(dec(60.0, 0), "60");
        assert_eq!(dec(0.053, 3), "0,053");
        assert_eq!(dec(-2.0, 2), "-2,00");
    }

    /// Ein Durchlauf ohne Fenster: Datumsfeld samt Kalender-Inhalt darf nicht abstürzen und ändert nichts ohne Klick.
    #[test]
    fn date_widgets_run_headless() {
        let ctx = egui::Context::default();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(TODAY_KEY), "2026-11-03".to_string()));
        let mut iso = "2026-02-14".to_string();
        let mut picked = true;
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    date_edit(ui, "test", &mut iso);
                    picked = calendar_menu(ui, egui::Id::new("cal_test"), &mut iso);
                });
            });
        }
        assert!(!picked);
        assert_eq!(iso, "2026-02-14");
        assert_eq!(year_month("2026-02-14"), Some((2026, 2)));
        assert_eq!(year_month("kaputt"), None);
    }
}
