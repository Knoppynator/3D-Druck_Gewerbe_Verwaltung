//! Farben, Abstände und Schriften der Oberfläche (siehe UI-KONZEPT.md). Hell und dunkel teilen sich alle
//! Bausteine; nur die Palette wechselt.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Alle Farben der Oberfläche für ein Design.
pub struct Palette {
    /// Seitenhintergrund
    pub page: Color32,
    /// Karten, Eingabefelder
    pub card: Color32,
    /// Navigation links, Statuszeile
    pub sidebar: Color32,
    /// Rahmen und Trennlinien
    pub line: Color32,
    /// leise Trennlinien in Karten, Hover in der Navigation
    pub line_soft: Color32,
    /// Rahmen von Eingabefeldern und Knöpfen
    pub field_line: Color32,
    /// Flächen von Bedienelementen (Häkchen, Regler) – Ruhe, Hover, gedrückt
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    /// leicht abgesetzte Fläche (gestreifte Tabellen, Zeitstrahl)
    pub faint: Color32,

    pub text: Color32,
    pub text_soft: Color32,
    pub muted: Color32,
    pub nav_text: Color32,

    pub accent: Color32,
    /// Akzent als Text (Links, gewählte Navigation)
    pub accent_text: Color32,
    /// Fläche der gewählten Zeile
    pub accent_soft: Color32,
    /// Spur von Messbalken, geplante Jobs
    pub accent_track: Color32,
    /// helle Akzentstufe (laufender Monat, geplante Jobs)
    pub accent_light: Color32,

    pub good: Color32,
    pub good_text: Color32,
    pub good_bg: Color32,
    pub warn: Color32,
    pub warn_text: Color32,
    pub warn_bg: Color32,
    pub warn_line: Color32,
    pub serious: Color32,
    pub serious_text: Color32,
    pub serious_bg: Color32,
    pub critical: Color32,
    pub critical_text: Color32,
    pub critical_bg: Color32,
    pub critical_line: Color32,
    pub info_text: Color32,
    pub info_bg: Color32,
    pub info_line: Color32,
    /// Neutraler Chip
    pub neutral_bg: Color32,
    pub neutral_dot: Color32,

    /// Kategorien in Diagrammen (Reihenfolge fest, nie zyklisch)
    pub series: [Color32; 4],
}

pub const LIGHT: Palette = Palette {
    page: rgb(0xf4f4f1),
    card: rgb(0xffffff),
    sidebar: rgb(0xfbfbf9),
    line: rgb(0xe3e2dc),
    line_soft: rgb(0xefeee9),
    field_line: rgb(0xd8d7cf),
    control: rgb(0xf1f0eb),
    control_hover: rgb(0xe9e8e2),
    control_active: rgb(0xdedcd5),
    faint: rgb(0xf7f6f2),
    text: rgb(0x1b1b19),
    text_soft: rgb(0x55544f),
    muted: rgb(0x6f6e68),
    nav_text: rgb(0x3a3935),
    accent: rgb(0x2a78d6),
    accent_text: rgb(0x1c5cab),
    accent_soft: rgb(0xe3eefb),
    accent_track: rgb(0xcde2fb),
    accent_light: rgb(0x9ec5f4),
    good: rgb(0x0ca30c),
    good_text: rgb(0x0a5c0a),
    good_bg: rgb(0xe7f5e7),
    warn: rgb(0xfab219),
    warn_text: rgb(0x6b4600),
    warn_bg: rgb(0xfff4dc),
    warn_line: rgb(0xf3d48c),
    serious: rgb(0xec835a),
    serious_text: rgb(0x8a3714),
    serious_bg: rgb(0xfdeee7),
    critical: rgb(0xd03b3b),
    critical_text: rgb(0xa32424),
    critical_bg: rgb(0xfbe9e9),
    critical_line: rgb(0xefbcbc),
    info_text: rgb(0x123f74),
    info_bg: rgb(0xeef4fc),
    info_line: rgb(0xb7d3f6),
    neutral_bg: rgb(0xf1f0eb),
    neutral_dot: rgb(0x9a998f),
    series: [rgb(0x2a78d6), rgb(0xeb6834), rgb(0x1baf7a), rgb(0xeda100)],
};

pub const DARK: Palette = Palette {
    page: rgb(0x161615),
    card: rgb(0x1f1f1d),
    sidebar: rgb(0x1a1a19),
    line: rgb(0x34342f),
    line_soft: rgb(0x2a2a27),
    field_line: rgb(0x45453f),
    control: rgb(0x2a2a27),
    control_hover: rgb(0x34342f),
    control_active: rgb(0x3e3e38),
    faint: rgb(0x232321),
    text: rgb(0xf0efec),
    text_soft: rgb(0xc3c2b7),
    muted: rgb(0x9a998f),
    nav_text: rgb(0xd6d5cc),
    accent: rgb(0x3987e5),
    accent_text: rgb(0x8dbcf2),
    accent_soft: rgb(0x1d3047),
    accent_track: rgb(0x223a57),
    accent_light: rgb(0x2b5a8f),
    good: rgb(0x0ca30c),
    good_text: rgb(0x6fd06f),
    good_bg: rgb(0x173317),
    warn: rgb(0xfab219),
    warn_text: rgb(0xf3c96a),
    warn_bg: rgb(0x3a2e12),
    warn_line: rgb(0x5c4719),
    serious: rgb(0xec835a),
    serious_text: rgb(0xf0a383),
    serious_bg: rgb(0x3b2318),
    critical: rgb(0xe66767),
    critical_text: rgb(0xf08a8a),
    critical_bg: rgb(0x3d1c1c),
    critical_line: rgb(0x5e2a2a),
    info_text: rgb(0xb7d3f6),
    info_bg: rgb(0x1a2a3d),
    info_line: rgb(0x2b4566),
    neutral_bg: rgb(0x2a2a27),
    neutral_dot: rgb(0x8a8980),
    series: [rgb(0x3987e5), rgb(0xd95926), rgb(0x199e70), rgb(0xc98500)],
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

/// Die Palette des aktuell gewählten Designs.
pub fn p() -> &'static Palette {
    if DARK_MODE.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

/// Design-Einstellung (gespeichert als `ui_theme`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
    /// folgt Windows
    System,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Light, Mode::Dark, Mode::System];
    pub const SETTING: &'static str = "ui_theme";

    pub fn key(self) -> &'static str {
        match self {
            Mode::Light => "hell",
            Mode::Dark => "dunkel",
            Mode::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Light => "Hell",
            Mode::Dark => "Dunkel",
            Mode::System => "Wie Windows",
        }
    }

    pub fn from_key(key: &str) -> Mode {
        Mode::ALL.into_iter().find(|m| m.key() == key).unwrap_or(Mode::Light)
    }

    /// Ob dunkel gezeichnet wird (bei `System` nach der Windows-Einstellung).
    pub fn is_dark(self, ctx: &egui::Context) -> bool {
        match self {
            Mode::Light => false,
            Mode::Dark => true,
            Mode::System => ctx.system_theme() == Some(egui::Theme::Dark),
        }
    }
}

/// Schriftfamilie für Titel, Kennzahlen und hervorgehobenen Text.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Text in der Halbfett-Schrift (egui kennt kein „fett“ innerhalb einer Familie).
pub fn strong(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).family(semibold()).size(size).color(p().text)
}

/// Segoe UI aus dem Windows-Schriftordner; fehlt sie, bleibt die egui-Standardschrift.
fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("WINDIR").map_or_else(|| "C:\\Windows".into(), std::path::PathBuf::from).join("Fonts");
    let mut semibold_chain = Vec::new();
    if let Ok(data) = std::fs::read(dir.join("segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), egui::FontData::from_owned(data).into());
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    if let Ok(data) = std::fs::read(dir.join("seguisb.ttf")) {
        fonts.font_data.insert("segoe-semibold".into(), egui::FontData::from_owned(data).into());
        semibold_chain.push("segoe-semibold".to_string());
    }
    // Rückfall auf die normale Schrift (Sonderzeichen, Symbole)
    semibold_chain.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(semibold(), semibold_chain);
    fonts
}

/// Schriften einmal beim Start setzen.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
}

/// Setzt das Design (Farben, Abstände) nach `mode`; gibt zurück, ob dunkel gezeichnet wird.
pub fn apply(ctx: &egui::Context, mode: Mode) -> bool {
    let dark = mode.is_dark(ctx);
    DARK_MODE.store(dark, Ordering::Relaxed);
    let (theme, pal) = if dark { (egui::Theme::Dark, &DARK) } else { (egui::Theme::Light, &LIGHT) };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |style| configure(style, pal, dark));
    dark
}

fn configure(style: &mut egui::Style, c: &Palette, dark: bool) {
    style.text_styles = [
        (TextStyle::Heading, FontId::new(22.0, semibold())),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 8.0);
    s.button_padding = egui::vec2(12.0, 5.0);
    s.interact_size.y = 30.0;
    s.window_margin = egui::Margin::same(16);
    s.menu_margin = egui::Margin::same(8);
    s.combo_width = 180.0;
    // Scrollbalken neben dem Inhalt statt darüber, mit Luft zu den Karten
    s.scroll = egui::style::ScrollStyle { bar_inner_margin: 10.0, bar_width: 8.0, ..egui::style::ScrollStyle::solid() };

    style.interaction.selectable_labels = false;

    let v = &mut style.visuals;
    v.dark_mode = dark;
    v.panel_fill = c.page;
    v.window_fill = c.card;
    v.extreme_bg_color = c.card;
    v.faint_bg_color = c.faint;
    v.code_bg_color = c.control;
    v.hyperlink_color = c.accent_text;
    v.warn_fg_color = c.warn_text;
    v.error_fg_color = c.critical_text;
    v.window_stroke = Stroke::new(1.0_f32, c.line);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    let shadow = Color32::from_black_alpha(if dark { 90 } else { 28 });
    v.window_shadow = egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: shadow };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: shadow };
    v.selection.bg_fill = c.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, c.accent_text);

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.card;
    w.noninteractive.weak_bg_fill = c.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = c.control;
    w.inactive.weak_bg_fill = c.card;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, c.field_line);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, c.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = c.control_hover;
    w.hovered.weak_bg_fill = c.control;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, c.muted);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, c.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = c.control_active;
    w.active.weak_bg_fill = c.control_hover;
    w.active.bg_stroke = Stroke::new(1.0_f32, c.accent);
    w.active.fg_stroke = Stroke::new(1.5_f32, c.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = c.control;
    w.open.weak_bg_fill = c.control;
    w.open.bg_stroke = Stroke::new(1.0_f32, c.accent);
    w.open.fg_stroke = Stroke::new(1.0_f32, c.text);
    w.open.corner_radius = radius;
}
