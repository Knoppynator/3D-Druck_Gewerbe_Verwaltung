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

/// Beschriftung links in einem Formular-Raster (leiser als der Inhalt).
pub fn form_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(theme::p().text_soft));
}

pub fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    form_label(ui, label);
    ui.add(egui::TextEdit::singleline(value).desired_width(320.0));
    ui.end_row();
}

pub fn int_row(ui: &mut egui::Ui, label: &str, value: &mut i64, suffix: &str) {
    form_label(ui, label);
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
    form_label(ui, label);
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
    form_label(ui, label);
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
        if primary_button(ui, "Speichern").clicked() {
            *act = Action::Save;
        }
        if can_delete && danger_button(ui, "Löschen").clicked() {
            *act = Action::Delete;
        }
    });
}

// ------------------------------------------------------------------ Gestaltungs-Bausteine

/// Weiße Karte mit feiner Linie – der Rahmen für jeden Bereich einer Seite.
pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::p().card)
        .stroke(egui::Stroke::new(1.0_f32, theme::p().line))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(18, 14))
}

/// Titelzeile einer Karte: Titel links, rechts leiser Hinweis und/oder Aktionen.
pub fn card_title(ui: &mut egui::Ui, title: &str, hint: &str, actions: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(theme::strong(title, 16.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            actions(ui);
            if !hint.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(hint).size(12.0).color(theme::p().muted)).truncate());
            }
        });
    });
    ui.add_space(4.0);
}

/// Karte mit Titel über die volle Breite; `hint` steht rechts im Titel.
pub fn card<R>(ui: &mut egui::Ui, title: &str, hint: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_actions(ui, title, hint, |_| {}, add)
}

/// Karte mit Titel, Hinweis und Aktionen rechts im Titel.
pub fn card_actions<R>(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    actions: impl FnOnce(&mut egui::Ui),
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !title.is_empty() {
            card_title(ui, title, hint, actions);
        }
        add(ui)
    });
    ui.add_space(4.0);
    inner.inner
}

/// Seitenkopf: Titel, eine Zeile Kontext und rechts die Hauptaktionen.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str, actions: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(theme::strong(title, 24.0));
            if !subtitle.is_empty() {
                ui.label(egui::RichText::new(subtitle).color(theme::p().text_soft));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
    ui.add_space(8.0);
}

/// Kleine Überschrift innerhalb einer Karte (z. B. „Stückliste“).
pub fn sub_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).family(theme::semibold()).color(theme::p().text));
}

/// Leiser Erklärungstext.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(theme::p().muted));
}

/// Kopfzeile einer Tabelle (`egui::Grid`), leise und halbfett.
pub fn table_head(ui: &mut egui::Ui, headers: &[&str]) {
    for h in headers {
        ui.label(egui::RichText::new(*h).size(13.0).family(theme::semibold()).color(theme::p().text_soft));
    }
    ui.end_row();
}

/// Bedeutung einer Status-Anzeige; Farbe nie allein, immer mit Text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    pub fn colors(self) -> (Color32, Color32, Color32) {
        let c = theme::p();
        match self {
            Tone::Neutral => (c.neutral_bg, c.nav_text, c.neutral_dot),
            Tone::Accent => (c.accent_soft, c.accent_text, c.accent),
            Tone::Good => (c.good_bg, c.good_text, c.good),
            Tone::Warn => (c.warn_bg, c.warn_text, c.warn),
            Tone::Serious => (c.serious_bg, c.serious_text, c.serious),
            Tone::Critical => (c.critical_bg, c.critical_text, c.critical),
        }
    }

    /// Textfarbe für Werte in diesem Ton (neutral = leiser Text).
    pub fn text(self) -> Color32 {
        match self {
            Tone::Neutral => theme::p().muted,
            _ => self.colors().1,
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
    let c = theme::p();
    let (bg, fg, line, icon, icon_color) = match tone {
        Tone::Critical => (c.critical_bg, c.critical_text, c.critical_line, icons::Icon::AlertCircle, c.critical),
        Tone::Warn | Tone::Serious => (c.warn_bg, c.warn_text, c.warn_line, icons::Icon::Alert, c.warn_text),
        Tone::Good => (c.good_bg, c.good_text, c.good_bg, icons::Icon::Check, c.good),
        Tone::Neutral | Tone::Accent => (c.info_bg, c.info_text, c.info_line, icons::Icon::Info, c.accent),
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
    ui.add_space(4.0);
    clicked
}

/// Kennzahl: Titel, Wert, Zusatzzeile (deren Ton färbt sie).
pub struct Stat {
    pub title: String,
    pub value: String,
    pub sub: String,
    pub tone: Tone,
}

impl Stat {
    pub fn new(title: impl Into<String>, value: impl Into<String>, sub: impl Into<String>) -> Stat {
        Stat { title: title.into(), value: value.into(), sub: sub.into(), tone: Tone::Neutral }
    }

    pub fn tone(mut self, tone: Tone) -> Stat {
        self.tone = tone;
        self
    }
}

fn stat_tile(ui: &mut egui::Ui, s: &Stat) {
    card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add(egui::Label::new(egui::RichText::new(&s.title).color(theme::p().text_soft)).truncate());
        ui.add(egui::Label::new(theme::strong(&s.value, 24.0)).truncate());
        ui.add(egui::Label::new(egui::RichText::new(&s.sub).size(12.0).color(s.tone.text())).truncate());
    });
}

/// Verteilt Kennzahl-Kacheln auf so viele Spalten, wie mit `min_width` nebeneinander passen.
pub fn stat_rows(ui: &mut egui::Ui, stats: &[Stat], min_width: f32) {
    let gap = ui.spacing().item_spacing.x;
    let per_row = (((ui.available_width() + gap) / (min_width + gap)).floor() as usize).clamp(1, stats.len().max(1));
    for row in stats.chunks(per_row) {
        ui.columns(per_row, |cols| {
            for (col, s) in cols.iter_mut().zip(row) {
                stat_tile(col, s);
            }
        });
    }
    ui.add_space(4.0);
}

/// Messbalken: gefüllter Anteil `fraction` (0–1) auf einer helleren Spur, optional ein Strich bei `marker`.
pub fn meter(ui: &mut egui::Ui, width: f32, fraction: f32, fill: Color32, track: Color32, marker: Option<f32>) -> egui::Response {
    let height = 8.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height + 8.0), egui::Sense::hover());
    let bar = egui::Rect::from_center_size(rect.center(), egui::vec2(width, height));
    let painter = ui.painter();
    painter.rect_filled(bar, 4.0, track);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let filled = egui::Rect::from_min_size(bar.min, egui::vec2((bar.width() * f).max(height), height));
        painter.rect_filled(filled, 4.0, fill);
    }
    if let Some(m) = marker {
        let x = bar.left() + bar.width() * m.clamp(0.0, 1.0);
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(2.0_f32, theme::p().text));
    }
    response
}

/// Hauptaktion (gefüllt im Akzent).
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let label = egui::RichText::new(text).family(theme::semibold()).color(Color32::WHITE);
    ui.add(egui::Button::new(label).fill(theme::p().accent).stroke(egui::Stroke::NONE))
}

/// Weiche Aktion (hellblau), z. B. „Einplanen“.
pub fn soft_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let c = theme::p();
    let label = egui::RichText::new(text).family(theme::semibold()).color(c.accent_text);
    ui.add(egui::Button::new(label).fill(c.info_bg).stroke(egui::Stroke::new(1.0_f32, c.info_line)))
}

/// Kleiner Knopf nur mit Symbol; der Tooltip sagt, was er tut.
pub fn icon_button(ui: &mut egui::Ui, icon: icons::Icon, hint: &str, color: Option<Color32>) -> egui::Response {
    let c = theme::p();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    let bg = if enabled && response.hovered() { c.control } else { c.card };
    ui.painter().rect(rect, 8.0, bg, egui::Stroke::new(1.0_f32, c.line), egui::StrokeKind::Inside);
    let mut ink = color.unwrap_or(c.nav_text);
    if !enabled {
        ink = ink.gamma_multiply(0.35);
    }
    icons::paint(ui.painter(), icon, egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0)), ink);
    response.on_hover_text(hint).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Löschen und andere Aktionen mit Folgen: normaler Knopf mit roter Schrift.
pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(text).color(theme::p().critical_text)))
}

/// Eintrag einer Auswahlliste (links auf Listen-/Detailseiten): Titel, rechts ein Wert, darunter Details und ein Chip.
pub struct ListEntry<'a> {
    pub title: &'a str,
    pub right: &'a str,
    pub meta: &'a str,
    pub chip: Option<(Tone, &'a str)>,
    /// zusätzliche Zeile in eigener Farbe (z. B. „Termin überschritten“)
    pub note: Option<(&'a str, Color32)>,
}

impl<'a> ListEntry<'a> {
    pub fn new(title: &'a str) -> ListEntry<'a> {
        ListEntry { title, right: "", meta: "", chip: None, note: None }
    }
}

/// Zeichnet einen Listeneintrag als Karte; Klick wählt ihn aus.
pub fn list_entry(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, selected: bool, e: &ListEntry<'_>) -> egui::Response {
    let id = ui.id().with(("list_entry", id_salt));
    let hovered = ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let c = theme::p();
    let (fill, stroke) = if selected {
        (c.accent_soft, c.accent_light)
    } else if hovered {
        (c.faint, c.field_line)
    } else {
        (c.card, c.line)
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0_f32, stroke))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal(|ui| {
                if !e.right.is_empty() {
                    // rechter Wert zuerst, damit ein langer Titel gekürzt wird und nicht den Wert verdrängt
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(e.right).family(theme::semibold()).color(c.text));
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(e.title).family(theme::semibold()).color(c.text)).truncate());
                        });
                    });
                } else {
                    ui.add(egui::Label::new(egui::RichText::new(e.title).family(theme::semibold()).color(c.text)).truncate());
                }
            });
            if !e.meta.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(e.meta).size(12.0).color(c.text_soft)).truncate());
            }
            if e.chip.is_some() || e.note.is_some() {
                ui.horizontal(|ui| {
                    if let Some((tone, text)) = e.chip {
                        chip(ui, tone, text);
                    }
                    if let Some((text, color)) = e.note {
                        ui.label(egui::RichText::new(text).size(12.0).color(color));
                    }
                });
            }
        });
    ui.add_space(2.0);
    ui.interact(inner.response.rect, id, egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Linke Listenspalte einer Listen-/Detailseite: eigene Breite, scrollt für sich.
pub fn list_panel(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    let frame = egui::Frame::new().fill(theme::p().page).inner_margin(egui::Margin { left: 0, right: 14, top: 0, bottom: 0 });
    egui::SidePanel::left(id.to_string())
        .default_width(310.0)
        .width_range(240.0..=460.0)
        .frame(frame)
        .show_inside(ui, add);
}

/// Rechter Detailbereich einer Listen-/Detailseite (scrollt).
pub fn detail_panel(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let frame = egui::Frame::new().fill(theme::p().page).inner_margin(egui::Margin { left: 10, right: 0, top: 0, bottom: 0 });
    egui::CentralPanel::default().frame(frame).show_inside(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, add);
    });
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
