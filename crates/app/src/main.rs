#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icons;
mod preview;
mod theme;
mod views;
mod widgets;

use std::path::PathBuf;

use druckplaner_core::{production::JobStatus, Db};
use eframe::egui::{self, Color32};
use icons::Icon;

use views::{
    assets::Assets,
    categories::Categories,
    customers::Customers,
    dashboard::DashboardView,
    euer::Euer,
    expenses::Expenses,
    invoices::Invoices,
    materials::Materials,
    orders::Orders,
    overview::{Nav, Overview},
    parts::{PrintParts, PurchaseParts},
    plates::Plates,
    production::Production,
    products::Products,
    settings::SettingsView,
    stock::StockView,
};

/// `DRUCKPLANER_DATA` überschreibt den Standardort `%APPDATA%\Druckplaner`.
fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("DRUCKPLANER_DATA") {
        return p.into();
    }
    match std::env::var_os("APPDATA") {
        Some(appdata) => PathBuf::from(appdata).join("Druckplaner"),
        None => PathBuf::from("data"),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Dashboard,
    Overview,
    Orders,
    Invoices,
    Stock,
    Production,
    Plates,
    Assets,
    Expenses,
    Euer,
    Categories,
    Customers,
    Materials,
    PrintParts,
    PurchaseParts,
    Products,
    Settings,
}

impl Page {
    const WORK: [Page; 6] = [Page::Dashboard, Page::Overview, Page::Orders, Page::Production, Page::Invoices, Page::Stock];
    const FINANCE: [Page; 3] = [Page::Expenses, Page::Assets, Page::Euer];
    const MASTER_DATA: [Page; 7] =
        [Page::Customers, Page::Products, Page::Materials, Page::PrintParts, Page::PurchaseParts, Page::Plates, Page::Categories];

    fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::Overview => "Offen",
            Page::Orders => "Bestellungen",
            Page::Invoices => "Rechnungen",
            Page::Stock => "Lager",
            Page::Production => "Produktion",
            Page::Plates => "Plattenvorlagen",
            Page::Assets => "Anlagen",
            Page::Expenses => "Ausgaben",
            Page::Euer => "EÜR",
            Page::Categories => "Kategorien",
            Page::Customers => "Kunden",
            Page::Materials => "Filament",
            Page::PrintParts => "Druckteile",
            Page::PurchaseParts => "Zukaufteile",
            Page::Products => "Produkte",
            Page::Settings => "Einstellungen",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Dashboard => Icon::Dashboard,
            Page::Overview => Icon::Inbox,
            Page::Orders => Icon::Cart,
            Page::Invoices => Icon::Receipt,
            Page::Stock => Icon::Warehouse,
            Page::Production => Icon::Printer,
            Page::Plates => Icon::Layers,
            Page::Assets => Icon::Wrench,
            Page::Expenses => Icon::Wallet,
            Page::Euer => Icon::Calculator,
            Page::Categories => Icon::Tag,
            Page::Customers => Icon::Users,
            Page::Materials => Icon::Spool,
            Page::PrintParts => Icon::Cube,
            Page::PurchaseParts => Icon::Nut,
            Page::Products => Icon::Package,
            Page::Settings => Icon::Settings,
        }
    }
}

/// Kennzeichen rechts in einer Navigationszeile.
enum Badge {
    None,
    /// Zahl in einer blauen Plakette (etwas zu tun)
    Count(usize),
    /// leiser Text (z. B. Anzahl offener Bestellungen)
    Text(String, Color32),
    /// farbiger Punkt (Zustand, Erklärung im Tooltip)
    Dot(Color32, &'static str),
}

/// Kennzahlen für die Navigation; werden beim Seitenwechsel und alle 30 s neu geladen, nicht in jedem Bild.
#[derive(Default)]
struct NavBadges {
    todo: usize,
    open_orders: i64,
    printing: bool,
    queued: usize,
    overdue: bool,
    low_stock: i64,
    last_backup: String,
}

impl NavBadges {
    /// Fehler beim Laden lassen die Kennzeichen einfach weg – die Seiten melden sie selbst.
    fn load(db: &Db) -> NavBadges {
        let mut b = NavBadges::default();
        if let Ok(o) = db.open_items() {
            b.todo = o.to_ship.len() + o.to_invoice.len() + o.to_collect.len() + o.low_stock.len();
        }
        if let Ok(t) = db.dashboard_tiles() {
            b.open_orders = t.open_orders;
            b.overdue = t.overdue_cents > 0;
            b.low_stock = t.low_stock;
        }
        if let Ok(jobs) = db.list_jobs(false) {
            b.printing = jobs.iter().any(|j| j.status == JobStatus::Printing);
            b.queued = jobs.len();
        }
        if let Some(latest) = db.list_backups().ok().and_then(|l| l.into_iter().next()) {
            b.last_backup = druckplaner_core::date::format_datetime_de(&latest.label());
        }
        b
    }

    fn badge(&self, page: Page) -> Badge {
        match page {
            Page::Overview if self.todo > 0 => Badge::Count(self.todo),
            Page::Orders if self.open_orders > 0 => Badge::Text(self.open_orders.to_string(), theme::MUTED),
            Page::Production if self.printing => Badge::Dot(theme::GOOD, "Druck läuft"),
            Page::Production if self.queued > 0 => Badge::Text(self.queued.to_string(), theme::MUTED),
            Page::Invoices if self.overdue => Badge::Text("überfällig".into(), theme::CRITICAL_TEXT),
            Page::Stock if self.low_stock > 0 => Badge::Dot(theme::WARN, "Posten unter Mindestbestand oder knapp"),
            _ => Badge::None,
        }
    }
}

/// Eine Zeile der Navigation: Symbol, Name und rechts das Kennzeichen.
fn nav_item(ui: &mut egui::Ui, page: Page, selected: bool, badge: Badge) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        theme::ACCENT_SOFT
    } else if response.hovered() {
        theme::LINE_SOFT
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 8.0, bg);
    let fg = if selected { theme::ACCENT_DARK } else { theme::NAV_TEXT };
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 19.0, rect.center().y), egui::vec2(18.0, 18.0));
    icons::paint(painter, page.icon(), icon_rect, fg);
    let font = egui::FontId::new(14.0, if selected { theme::semibold() } else { egui::FontFamily::Proportional });
    painter.text(egui::pos2(rect.left() + 38.0, rect.center().y), egui::Align2::LEFT_CENTER, page.title(), font, fg);

    let right = egui::pos2(rect.right() - 10.0, rect.center().y);
    let mut tooltip = None;
    match badge {
        Badge::None => {}
        Badge::Count(n) => {
            let galley = painter.layout_no_wrap(n.to_string(), egui::FontId::new(11.0, theme::semibold()), Color32::WHITE);
            let w = (galley.size().x + 12.0).max(20.0);
            let pill = egui::Rect::from_min_max(egui::pos2(right.x - w, right.y - 10.0), egui::pos2(right.x, right.y + 10.0));
            painter.rect_filled(pill, 10.0, theme::ACCENT);
            painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
        }
        Badge::Text(text, color) => {
            painter.text(right, egui::Align2::RIGHT_CENTER, text, egui::FontId::new(12.0, theme::semibold()), color);
        }
        Badge::Dot(color, hint) => {
            painter.circle_filled(egui::pos2(right.x - 4.0, right.y), 4.0, color);
            tooltip = Some(hint);
        }
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    match tooltip {
        Some(hint) => response.on_hover_text(hint),
        None => response,
    }
}

fn nav_group(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(11.0).family(theme::semibold()).color(theme::MUTED));
    });
}

struct Main {
    db: Db,
    page: Page,
    status: String,
    dashboard: DashboardView,
    overview: Overview,
    orders: Orders,
    invoices: Invoices,
    stock: StockView,
    production: Production,
    plates: Plates,
    assets: Assets,
    expenses: Expenses,
    euer: Euer,
    categories: Categories,
    customers: Customers,
    materials: Materials,
    print_parts: PrintParts,
    purchase_parts: PurchaseParts,
    products: Products,
    settings: SettingsView,
    badges: NavBadges,
    /// Zeitpunkt (egui-Zeit in s) des letzten Ladens der Kennzeichen
    badges_at: f64,
}

impl Main {
    fn new(dir: PathBuf) -> Result<Main, String> {
        let db = Db::open(&dir.join("druckplaner.sqlite")).map_err(|e| e.to_string())?;
        let mut main = Main {
            db,
            page: Page::Dashboard,
            status: format!("Daten: {}", dir.display()),
            dashboard: DashboardView::default(),
            overview: Overview::default(),
            orders: Orders::default(),
            invoices: Invoices::default(),
            stock: StockView::default(),
            production: Production::default(),
            plates: Plates::default(),
            assets: Assets::default(),
            expenses: Expenses::default(),
            euer: Euer::default(),
            categories: Categories::default(),
            customers: Customers::default(),
            materials: Materials::default(),
            print_parts: PrintParts::default(),
            purchase_parts: PurchaseParts::default(),
            products: Products::default(),
            settings: SettingsView::new(dir),
            badges: NavBadges::default(),
            badges_at: f64::NEG_INFINITY,
        };
        main.set_page(Page::Dashboard);
        Ok(main)
    }

    /// Wechselt die Seite und lädt deren Daten frisch (Auswahllisten können sich geändert haben).
    fn set_page(&mut self, page: Page) {
        self.page = page;
        // Kennzeichen der Navigation beim nächsten Bild neu laden
        self.badges_at = f64::NEG_INFINITY;
        match page {
            Page::Dashboard => self.dashboard.reload(&self.db, &mut self.status),
            Page::Overview => self.overview.reload(&self.db, &mut self.status),
            Page::Orders => self.orders.reload(&self.db, &mut self.status),
            Page::Invoices => self.invoices.reload(&self.db, &mut self.status),
            Page::Stock => self.stock.reload(&self.db, &mut self.status),
            Page::Production => self.production.reload(&self.db, &mut self.status),
            Page::Plates => self.plates.reload(&self.db, &mut self.status),
            Page::Assets => self.assets.reload(&self.db, &mut self.status),
            Page::Expenses => self.expenses.reload(&self.db, &mut self.status),
            Page::Euer => self.euer.reload(&self.db, &mut self.status),
            Page::Categories => self.categories.reload(&self.db, &mut self.status),
            Page::Customers => self.customers.reload(&self.db, &mut self.status),
            Page::Materials => self.materials.reload(&self.db, &mut self.status),
            Page::PrintParts => self.print_parts.reload(&self.db, &mut self.status),
            Page::PurchaseParts => self.purchase_parts.reload(&self.db, &mut self.status),
            Page::Products => self.products.reload(&self.db, &mut self.status),
            Page::Settings => self.settings.reload(&self.db, &mut self.status),
        }
    }

    fn go(&mut self, nav: Nav) {
        match nav {
            Nav::Order(id) => {
                self.set_page(Page::Orders);
                self.orders.open(&self.db, &mut self.status, id);
            }
            Nav::Invoice(id) => {
                self.set_page(Page::Invoices);
                self.invoices.select(&self.db, &mut self.status, id);
            }
            Nav::Stock(key) => {
                self.set_page(Page::Stock);
                self.stock.select(&self.db, &mut self.status, key);
            }
        }
    }

    fn show(&mut self, ctx: &egui::Context) {
        // heutiges Datum für den Kalender in den Datumsfeldern
        if let Ok(today) = self.db.today() {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(widgets::TODAY_KEY), today));
        }
        let now = ctx.input(|i| i.time);
        if now - self.badges_at > 30.0 {
            self.badges = NavBadges::load(&self.db);
            self.badges_at = now;
        }
        // alle 30 s auch ohne Eingabe neu zeichnen, damit die Kennzeichen aktuell bleiben
        ctx.request_repaint_after(std::time::Duration::from_secs(30));

        let status_frame = egui::Frame::new().fill(theme::SIDEBAR).inner_margin(egui::Margin::symmetric(14, 5));
        egui::TopBottomPanel::bottom("status_bar").frame(status_frame).show(ctx, |ui| {
            let color = if self.status.starts_with("Fehler") { theme::CRITICAL_TEXT } else { theme::TEXT_SOFT };
            ui.label(egui::RichText::new(&self.status).size(12.0).color(color));
        });

        let mut goto = None;
        let nav_frame = egui::Frame::new().fill(theme::SIDEBAR).inner_margin(egui::Margin { left: 10, right: 10, top: 14, bottom: 10 });
        egui::SidePanel::left("nav").exact_width(224.0).resizable(false).frame(nav_frame).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if let Some(tex) = icons::app_texture(ctx) {
                    ui.add(egui::Image::new((tex, egui::vec2(32.0, 32.0))));
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(theme::strong("Druckplaner", 16.0));
                    ui.label(egui::RichText::new("3D-Druck-Fertigung").size(12.0).color(theme::MUTED));
                });
            });
            ui.add_space(6.0);

            // Platz unten für Sicherungsstatus und Einstellungen freihalten
            let bottom = 76.0;
            egui::ScrollArea::vertical().max_height((ui.available_height() - bottom).max(80.0)).show(ui, |ui| {
                for (title, pages) in [("ARBEIT", &Page::WORK[..]), ("FINANZEN", &Page::FINANCE[..]), ("STAMMDATEN", &Page::MASTER_DATA[..])] {
                    nav_group(ui, title);
                    for &page in pages {
                        if nav_item(ui, page, self.page == page, self.badges.badge(page)).clicked() {
                            goto = Some(page);
                        }
                    }
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                if nav_item(ui, Page::Settings, self.page == Page::Settings, Badge::None).clicked() {
                    goto = Some(Page::Settings);
                }
                ui.add_space(4.0);
                if !self.badges.last_backup.is_empty() {
                    egui::Frame::new().fill(theme::LINE_SOFT).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            icons::show(ui, Icon::Check, 14.0, theme::GOOD_TEXT);
                            ui.label(egui::RichText::new(format!("Gesichert {}", self.badges.last_backup)).size(12.0).color(theme::TEXT_SOFT));
                        });
                    });
                }
            });
        });
        if let Some(page) = goto {
            self.set_page(page);
        }

        let mut nav = None;
        let page_frame = egui::Frame::new().fill(theme::PAGE).inner_margin(egui::Margin { left: 24, right: 24, top: 18, bottom: 10 });
        egui::CentralPanel::default().frame(page_frame).show(ctx, |ui| match self.page {
            Page::Dashboard => self.dashboard.ui(ui),
            Page::Overview => nav = self.overview.ui(ui, &self.db, &mut self.status),
            Page::Orders => self.orders.ui(ui, &self.db, &mut self.status),
            Page::Invoices => self.invoices.ui(ui, &self.db, &mut self.status),
            Page::Stock => self.stock.ui(ui, &self.db, &mut self.status),
            Page::Production => self.production.ui(ui, &self.db, &mut self.status),
            Page::Plates => self.plates.ui(ui, &self.db, &mut self.status),
            Page::Assets => self.assets.ui(ui, &self.db, &mut self.status),
            Page::Expenses => self.expenses.ui(ui, &self.db, &mut self.status),
            Page::Euer => self.euer.ui(ui, &self.db, &mut self.status),
            Page::Categories => self.categories.ui(ui, &self.db, &mut self.status),
            Page::Customers => self.customers.ui(ui, &self.db, &mut self.status),
            Page::Materials => self.materials.ui(ui, &self.db, &mut self.status),
            Page::PrintParts => self.print_parts.ui(ui, &self.db, &mut self.status),
            Page::PurchaseParts => self.purchase_parts.ui(ui, &self.db, &mut self.status),
            Page::Products => self.products.ui(ui, &self.db, &mut self.status),
            Page::Settings => self.settings.ui(ui, &self.db, &mut self.status),
        });
        if let Some(nav) = nav {
            self.go(nav);
        }
    }
}

struct App {
    inner: Result<Main, String>,
}

impl Main {
    /// Automatische Sicherung beim Beenden; Fehler landen in `backup.log` im Datenordner (das Fenster ist dann weg).
    fn backup_on_exit(&self) {
        if let Err(e) = self.db.auto_backup_if_needed() {
            let line = format!("{} Backup fehlgeschlagen: {e}\n", self.db.now().unwrap_or_default());
            let log = self.db.data_dir().join("backup.log");
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)
                .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
        }
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(main) = &self.inner {
            main.backup_on_exit();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match &mut self.inner {
            Ok(main) => main.show(ctx),
            Err(msg) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.heading("Datenbank konnte nicht geöffnet werden");
                    ui.label(msg.as_str());
                });
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Druckplaner")
            .with_inner_size([1320.0, 860.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(std::sync::Arc::new(icons::app_icon_data())),
        ..Default::default()
    };
    eframe::run_native(
        "Druckplaner",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            icons::init(&cc.egui_ctx);
            Ok(Box::new(App { inner: Main::new(data_dir()) }))
        }),
    )
}
