//! Farben, Abstände und Schriften der Oberfläche (helles Design, siehe UI-KONZEPT.md).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Seitenhintergrund
pub const PAGE: Color32 = rgb(0xf4f4f1);
/// Karten, Eingabefelder
pub const CARD: Color32 = rgb(0xffffff);
/// Navigation links
pub const SIDEBAR: Color32 = rgb(0xfbfbf9);
/// Rahmen und Trennlinien
pub const LINE: Color32 = rgb(0xe3e2dc);
/// Leise Trennlinien innerhalb von Karten, Hover in der Navigation
pub const LINE_SOFT: Color32 = rgb(0xefeee9);
/// Rahmen von Eingabefeldern und Knöpfen
pub const FIELD_LINE: Color32 = rgb(0xd8d7cf);

pub const TEXT: Color32 = rgb(0x1b1b19);
pub const TEXT_SOFT: Color32 = rgb(0x55544f);
pub const MUTED: Color32 = rgb(0x6f6e68);
pub const NAV_TEXT: Color32 = rgb(0x3a3935);

pub const ACCENT: Color32 = rgb(0x2a78d6);
pub const ACCENT_DARK: Color32 = rgb(0x1c5cab);
pub const ACCENT_SOFT: Color32 = rgb(0xe3eefb);

pub const GOOD: Color32 = rgb(0x0ca30c);
pub const GOOD_TEXT: Color32 = rgb(0x0a5c0a);
pub const GOOD_BG: Color32 = rgb(0xe7f5e7);
pub const WARN: Color32 = rgb(0xfab219);
pub const WARN_TEXT: Color32 = rgb(0x6b4600);
pub const WARN_BG: Color32 = rgb(0xfff4dc);
pub const WARN_LINE: Color32 = rgb(0xf3d48c);
pub const SERIOUS: Color32 = rgb(0xec835a);
pub const SERIOUS_TEXT: Color32 = rgb(0x8a3714);
pub const SERIOUS_BG: Color32 = rgb(0xfdeee7);
pub const CRITICAL: Color32 = rgb(0xd03b3b);
pub const CRITICAL_TEXT: Color32 = rgb(0xa32424);
pub const CRITICAL_BG: Color32 = rgb(0xfbe9e9);
pub const CRITICAL_LINE: Color32 = rgb(0xefbcbc);

/// Schriftfamilie für Titel, Kennzahlen und hervorgehobenen Text.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Text in der Halbfett-Schrift (egui kennt kein „fett“ innerhalb einer Familie).
pub fn strong(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).family(semibold()).size(size).color(TEXT)
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

/// Setzt Schriften, Farben und Abstände einmal beim Start.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::ThemePreference::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
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

        let v = &mut style.visuals;
        v.panel_fill = PAGE;
        v.window_fill = CARD;
        v.extreme_bg_color = CARD;
        v.faint_bg_color = rgb(0xf7f6f2);
        v.code_bg_color = rgb(0xf1f0eb);
        v.hyperlink_color = ACCENT_DARK;
        v.warn_fg_color = WARN_TEXT;
        v.error_fg_color = CRITICAL_TEXT;
        v.window_stroke = Stroke::new(1.0_f32, LINE);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_shadow = egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(28) };
        v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(24) };
        v.selection.bg_fill = ACCENT_SOFT;
        v.selection.stroke = Stroke::new(1.0_f32, ACCENT_DARK);

        let radius = CornerRadius::same(8);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = CARD;
        w.noninteractive.weak_bg_fill = CARD;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.noninteractive.corner_radius = radius;

        w.inactive.bg_fill = rgb(0xf1f0eb);
        w.inactive.weak_bg_fill = CARD;
        w.inactive.bg_stroke = Stroke::new(1.0_f32, FIELD_LINE);
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = radius;

        w.hovered.bg_fill = rgb(0xe9e8e2);
        w.hovered.weak_bg_fill = rgb(0xf3f2ee);
        w.hovered.bg_stroke = Stroke::new(1.0_f32, rgb(0xb9b8b0));
        w.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.hovered.corner_radius = radius;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = rgb(0xdedcd5);
        w.active.weak_bg_fill = rgb(0xe9e8e2);
        w.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
        w.active.corner_radius = radius;
        w.active.expansion = 0.0;

        w.open.bg_fill = rgb(0xf1f0eb);
        w.open.weak_bg_fill = rgb(0xf3f2ee);
        w.open.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        w.open.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.open.corner_radius = radius;
    });
}
