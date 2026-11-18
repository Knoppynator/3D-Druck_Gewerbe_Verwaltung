//! Linien-Symbole (Lucide-Stil, ISC-Lizenz; Drucker und Zukaufteil eigene) und das App-Symbol.
//!
//! Die SVGs werden beim Start einmal mit `resvg` zu weißen Masken gerastert und als Texturen geladen; beim Zeichnen
//! färbt `tint` sie ein.

use std::sync::OnceLock;

use eframe::egui::{self, Color32};
use resvg::{tiny_skia, usvg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Dashboard,
    Inbox,
    Cart,
    Printer,
    Receipt,
    Warehouse,
    Wallet,
    Wrench,
    Calculator,
    Users,
    Package,
    Spool,
    Cube,
    Nut,
    Layers,
    Tag,
    Settings,
    Check,
    Alert,
    AlertCircle,
    Info,
    ChevronUp,
    ChevronDown,
    Play,
    Trash,
    Plus,
    Search,
    Truck,
    Calendar,
}

const ALL: [Icon; 29] = [
    Icon::Dashboard,
    Icon::Inbox,
    Icon::Cart,
    Icon::Printer,
    Icon::Receipt,
    Icon::Warehouse,
    Icon::Wallet,
    Icon::Wrench,
    Icon::Calculator,
    Icon::Users,
    Icon::Package,
    Icon::Spool,
    Icon::Cube,
    Icon::Nut,
    Icon::Layers,
    Icon::Tag,
    Icon::Settings,
    Icon::Check,
    Icon::Alert,
    Icon::AlertCircle,
    Icon::Info,
    Icon::ChevronUp,
    Icon::ChevronDown,
    Icon::Play,
    Icon::Trash,
    Icon::Plus,
    Icon::Search,
    Icon::Truck,
    Icon::Calendar,
];

impl Icon {
    /// Innerer Teil des SVGs (24×24, Linien).
    fn paths(self) -> &'static str {
        match self {
            Icon::Dashboard => r#"<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>"#,
            Icon::Inbox => r#"<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>"#,
            Icon::Cart => r#"<circle cx="8" cy="21" r="1"/><circle cx="19" cy="21" r="1"/><path d="M2.05 2.05h2l2.66 12.42a2 2 0 0 0 2 1.58h9.78a2 2 0 0 0 1.95-1.57l1.65-7.43H5.12"/>"#,
            Icon::Printer => r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 8h18"/><path d="M10 8v3l2 2 2-2V8"/><path d="M7 17h10"/>"#,
            Icon::Receipt => r#"<path d="M4 2v20l2-1 2 1 2-1 2 1 2-1 2 1 2-1 2 1V2l-2 1-2-1-2 1-2-1-2 1-2-1-2 1Z"/><path d="M16 8h-6a2 2 0 1 0 0 4h4a2 2 0 1 1 0 4H8"/><path d="M12 17.5v-11"/>"#,
            Icon::Warehouse => r#"<path d="M22 8.35V20a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8.35A2 2 0 0 1 3.26 6.5l8-3.2a2 2 0 0 1 1.48 0l8 3.2A2 2 0 0 1 22 8.35Z"/><path d="M6 18h12"/><path d="M6 14h12"/><rect x="6" y="10" width="12" height="12"/>"#,
            Icon::Wallet => r#"<path d="M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 0 0 0 4h3a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1"/><path d="M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4"/>"#,
            Icon::Wrench => r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#,
            Icon::Calculator => r#"<rect x="4" y="2" width="16" height="20" rx="2"/><path d="M8 6h8"/><path d="M16 14v4"/><path d="M16 10h.01"/><path d="M12 10h.01"/><path d="M8 10h.01"/><path d="M12 14h.01"/><path d="M8 14h.01"/><path d="M12 18h.01"/><path d="M8 18h.01"/>"#,
            Icon::Users => r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#,
            Icon::Package => r#"<path d="m7.5 4.27 9 5.15"/><path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>"#,
            Icon::Spool => r#"<circle cx="12" cy="12" r="10"/><path d="M6 12c0-1.7.7-3.2 1.8-4.2"/><circle cx="12" cy="12" r="2"/><path d="M18 12c0 1.7-.7 3.2-1.8 4.2"/>"#,
            Icon::Cube => r#"<path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z"/><path d="m3.3 7 8.7 5 8.7-5"/><path d="M12 22V12"/>"#,
            Icon::Nut => r#"<path d="M12 2.5 20.2 7.25v9.5L12 21.5l-8.2-4.75v-9.5z"/><circle cx="12" cy="12" r="3.2"/>"#,
            Icon::Layers => r#"<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/>"#,
            Icon::Tag => r#"<path d="M12.59 2.59A2 2 0 0 0 11.17 2H4a2 2 0 0 0-2 2v7.17a2 2 0 0 0 .59 1.42l8.7 8.7a2.43 2.43 0 0 0 3.42 0l6.58-6.58a2.43 2.43 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r="1"/>"#,
            Icon::Settings => r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>"#,
            Icon::Check => r#"<path d="M20 6 9 17l-5-5"/>"#,
            Icon::Alert => r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#,
            Icon::AlertCircle => r#"<circle cx="12" cy="12" r="9"/><path d="M12 8v5"/><path d="M12 16h.01"/>"#,
            Icon::Info => r#"<circle cx="12" cy="12" r="9"/><path d="M12 16v-5"/><path d="M12 8h.01"/>"#,
            Icon::ChevronUp => r#"<path d="m18 15-6-6-6 6"/>"#,
            Icon::ChevronDown => r#"<path d="m6 9 6 6 6-6"/>"#,
            Icon::Play => r##"<path d="M7 4.5v15l12-7.5z" fill="#ffffff" stroke="none"/>"##,
            Icon::Trash => r#"<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#,
            Icon::Plus => r#"<path d="M12 5v14M5 12h14"/>"#,
            Icon::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#,
            Icon::Truck => r#"<path d="M14 18V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v11a1 1 0 0 0 1 1h2"/><path d="M15 18H9"/><path d="M19 18h2a1 1 0 0 0 1-1v-3.65a1 1 0 0 0-.22-.62l-3.48-4.35A1 1 0 0 0 17.52 8H14"/><circle cx="17" cy="18" r="2"/><circle cx="7" cy="18" r="2"/>"#,
            Icon::Calendar => r#"<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>"#,
        }
    }
}

/// Kantenlänge der gerasterten Masken; reicht für 24 px Anzeige bei 200 % Skalierung.
const RASTER: u32 = 64;

/// App-Symbol: Düse über drei Schichten (Schicht für Schicht).
pub const APP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#2a78d6"/><path d="M25 9h14v7l-7 7-7-7z" fill="#ffffff"/><rect x="20" y="29" width="24" height="6" rx="3" fill="#ffffff" fill-opacity="0.65"/><rect x="15" y="38" width="34" height="6" rx="3" fill="#ffffff" fill-opacity="0.82"/><rect x="10" y="47" width="44" height="6" rx="3" fill="#ffffff"/></svg>"##;

/// Rastert ein SVG auf `size`×`size` Pixel (vormultipliziertes RGBA).
fn rasterize(svg: &str, size: u32) -> Option<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(pixmap)
}

fn mask_svg(icon: Icon) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
        icon.paths()
    )
}

static TEXTURES: OnceLock<Vec<egui::TextureHandle>> = OnceLock::new();

/// Lädt alle Symbole als Texturen (einmal beim Start).
pub fn init(ctx: &egui::Context) {
    TEXTURES.get_or_init(|| {
        ALL.iter()
            .map(|&icon| {
                let image = match rasterize(&mask_svg(icon), RASTER) {
                    Some(p) => egui::ColorImage::from_rgba_premultiplied([RASTER as usize; 2], p.data()),
                    None => egui::ColorImage::filled([RASTER as usize; 2], Color32::TRANSPARENT),
                };
                ctx.load_texture(format!("icon_{icon:?}"), image, egui::TextureOptions::LINEAR)
            })
            .collect()
    });
}

fn texture(icon: Icon) -> Option<egui::TextureId> {
    let index = ALL.iter().position(|&i| i == icon)?;
    TEXTURES.get().map(|t| t[index].id())
}

/// Malt ein Symbol in `rect` in der Farbe `color`.
pub fn paint(painter: &egui::Painter, icon: Icon, rect: egui::Rect, color: Color32) {
    if let Some(id) = texture(icon) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(id, rect, uv, color);
    }
}

/// Symbol als eigenes Widget (`size` in Punkten).
pub fn show(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), icon, rect, color);
    response
}

/// App-Symbol als Textur (für die Navigation); wird beim ersten Aufruf erzeugt.
pub fn app_texture(ctx: &egui::Context) -> Option<egui::TextureId> {
    static APP: OnceLock<Option<egui::TextureHandle>> = OnceLock::new();
    APP.get_or_init(|| {
        let p = rasterize(APP_SVG, 128)?;
        let image = egui::ColorImage::from_rgba_premultiplied([128, 128], p.data());
        Some(ctx.load_texture("app_icon", image, egui::TextureOptions::LINEAR))
    })
    .as_ref()
    .map(egui::TextureHandle::id)
}

/// App-Symbol für Fenster und Taskleiste (nicht vormultipliziertes RGBA).
pub fn app_icon_data() -> egui::IconData {
    const SIZE: u32 = 256;
    let rgba = rasterize(APP_SVG, SIZE)
        .map(|p| {
            p.pixels()
                .iter()
                .flat_map(|px| {
                    let c = px.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                })
                .collect()
        })
        .unwrap_or_default();
    egui::IconData { rgba, width: SIZE, height: SIZE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_rasterizes_to_visible_pixels() {
        for icon in ALL {
            let pixmap = rasterize(&mask_svg(icon), RASTER).unwrap_or_else(|| panic!("{icon:?} lässt sich nicht lesen"));
            let inked = pixmap.pixels().iter().filter(|p| p.alpha() > 128).count();
            assert!(inked > 40, "{icon:?} ist (fast) leer: {inked} Pixel");
        }
    }

    #[test]
    fn app_icon_has_the_full_size() {
        let icon = app_icon_data();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Mitte unten ist eine weiße Schicht, die Ecke oben links transparent (abgerundet)
        let at = |x: usize, y: usize| &icon.rgba[(y * 256 + x) * 4..(y * 256 + x) * 4 + 4];
        assert_eq!(at(128, 200), &[255, 255, 255, 255]);
        assert_eq!(at(0, 0)[3], 0);
    }
}
