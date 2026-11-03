#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod preview;
mod views;
mod widgets;

use std::path::PathBuf;

use druckplaner_core::Db;
use eframe::egui;

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
        };
        main.set_page(Page::Dashboard);
        Ok(main)
    }

    /// Wechselt die Seite und lädt deren Daten frisch (Auswahllisten können sich geändert haben).
    fn set_page(&mut self, page: Page) {
        self.page = page;
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
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.label(&self.status);
        });

        let mut goto = None;
        egui::SidePanel::left("nav").exact_width(170.0).show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading("Druckplaner");
            ui.separator();
            for page in Page::WORK {
                if ui.selectable_label(self.page == page, page.title()).clicked() {
                    goto = Some(page);
                }
            }
            ui.add_space(8.0);
            ui.weak("Finanzen");
            for page in Page::FINANCE {
                if ui.selectable_label(self.page == page, page.title()).clicked() {
                    goto = Some(page);
                }
            }
            ui.add_space(8.0);
            ui.weak("Stammdaten");
            for page in Page::MASTER_DATA {
                if ui.selectable_label(self.page == page, page.title()).clicked() {
                    goto = Some(page);
                }
            }
            ui.add_space(8.0);
            if ui.selectable_label(self.page == Page::Settings, Page::Settings.title()).clicked() {
                goto = Some(Page::Settings);
            }
        });
        if let Some(page) = goto {
            self.set_page(page);
        }

        let mut nav = None;
        egui::CentralPanel::default().show(ctx, |ui| match self.page {
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
            .with_inner_size([1200.0, 780.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Druckplaner",
        options,
        Box::new(|_cc| Ok(Box::new(App { inner: Main::new(data_dir()) }))),
    )
}
