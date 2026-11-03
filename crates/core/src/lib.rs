//! Fachlogik und Datenhaltung des Druckplaners (ohne UI-Abhängigkeit).

pub mod assets;
pub mod backup;
pub mod board;
pub mod catalog;
pub mod dashboard;
pub mod date;
pub mod db;
pub mod error;
pub mod finance;
pub mod import;
pub mod invoicing;
pub mod money;
pub mod orders;
pub mod overview;
pub mod pdf;
pub mod pricing;
pub mod settings;
pub mod production;
pub mod shipments;
pub mod stock;
pub mod threemf;

pub use db::Db;
pub use error::{Error, Result};
