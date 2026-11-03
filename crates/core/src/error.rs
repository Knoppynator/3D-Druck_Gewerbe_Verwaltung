#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Datenbankfehler: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Dateifehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Übersetzt eine verletzte Datenbank-Bedingung (UNIQUE, FOREIGN KEY, CHECK) in eine lesbare Meldung.
pub(crate) fn map_constraint(e: rusqlite::Error, msg: &str) -> Error {
    match &e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            Error::Invalid(msg.to_string())
        }
        _ => Error::Db(e),
    }
}
