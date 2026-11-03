//! Seitenvorschau für PDF-Belege: Rendern auf der CPU mit `hayro`, Anzeige als Textur.

use std::path::{Path, PathBuf};

use eframe::egui;
use hayro::{
    hayro_interpret::InterpreterSettings, hayro_syntax::Pdf, vello_cpu::color::palette::css::WHITE, RenderCache,
    RenderSettings,
};

/// Breite der gerenderten Seite in Pixeln (die Anzeige skaliert auf die verfügbare Breite).
const RENDER_WIDTH: f32 = 1000.0;

pub struct PdfPreview {
    path: PathBuf,
    page: usize,
    pages: usize,
    texture: Option<egui::TextureHandle>,
    error: String,
}

/// Rendert Seite `page` (0-basiert) und gibt Bild und Seitenzahl zurück.
fn render_page(path: &Path, page: usize) -> Result<(egui::ColorImage, usize), String> {
    let data = std::fs::read(path).map_err(|e| format!("Datei nicht lesbar: {e}"))?;
    let pdf = Pdf::new(data).map_err(|e| format!("Kein lesbares PDF: {e:?}"))?;
    let pages = pdf.pages();
    let count = pages.len();
    let first = pages.get(page.min(count.saturating_sub(1))).ok_or("Das PDF hat keine Seiten")?;
    let (width, _) = first.render_dimensions();
    let scale = if width > 0.0 { RENDER_WIDTH / width } else { 1.0 };
    let settings = RenderSettings { x_scale: scale, y_scale: scale, bg_color: WHITE, ..Default::default() };
    let pixmap = hayro::render(first, &RenderCache::new(), &InterpreterSettings::default(), &settings);
    let size = [usize::from(pixmap.width()), usize::from(pixmap.height())];
    Ok((egui::ColorImage::from_rgba_premultiplied(size, pixmap.data_as_u8_slice()), count))
}

impl PdfPreview {
    pub fn new(ctx: &egui::Context, path: &Path) -> Self {
        let mut preview = PdfPreview { path: path.to_path_buf(), page: 0, pages: 0, texture: None, error: String::new() };
        preview.render(ctx);
        preview
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn render(&mut self, ctx: &egui::Context) {
        // Fremde PDFs können den Renderer zum Absturz bringen: das darf nicht die ganze App mitreißen
        let (path, page) = (self.path.clone(), self.page);
        let result = std::panic::catch_unwind(move || render_page(&path, page))
            .unwrap_or_else(|_| Err("Das PDF lässt sich nicht darstellen".to_string()));
        match result {
            Ok((image, pages)) => {
                self.pages = pages;
                self.texture = Some(ctx.load_texture("receipt_preview", image, egui::TextureOptions::LINEAR));
                self.error.clear();
            }
            Err(message) => {
                self.texture = None;
                self.error = message;
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.pages > 1 {
            ui.horizontal(|ui| {
                let before = self.page;
                if ui.add_enabled(self.page > 0, egui::Button::new("‹").small()).clicked() {
                    self.page -= 1;
                }
                ui.label(format!("Seite {} von {}", self.page + 1, self.pages));
                if ui.add_enabled(self.page + 1 < self.pages, egui::Button::new("›").small()).clicked() {
                    self.page += 1;
                }
                if self.page != before {
                    self.render(ui.ctx());
                }
            });
        }
        if !self.error.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, self.error.as_str());
        }
        if let Some(texture) = &self.texture {
            egui::ScrollArea::both().show(ui, |ui| {
                ui.add(egui::Image::new(texture).max_width(ui.available_width()));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendert einen echten Beleg aus `ressourcen/`, sofern vorhanden, und prüft, dass die Seite nicht leer bleibt.
    #[test]
    fn renders_a_real_receipt_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ressourcen/Ausgaben/Rechnungen/Amazon/invoice.pdf");
        if !path.exists() {
            return;
        }
        let (image, pages) = render_page(&path, 0).unwrap();
        assert!(pages >= 1);
        assert!(image.size[0] > 500 && image.size[1] > 500);
        let inked = image.pixels.iter().filter(|c| c.r() < 200).count();
        assert!(inked > 1000, "Seite wirkt leer ({inked} dunkle Pixel)");
    }

    #[test]
    fn broken_files_report_an_error_instead_of_panicking() {
        let dir = std::env::temp_dir().join(format!("druckplaner_preview_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("kaputt.pdf");
        std::fs::write(&file, b"kein pdf").unwrap();
        let preview = PdfPreview::new(&egui::Context::default(), &file);
        assert!(!preview.error.is_empty());
        assert!(preview.texture.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
