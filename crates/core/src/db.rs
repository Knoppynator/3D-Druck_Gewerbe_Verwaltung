use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::Connection;

use crate::error::Result;

/// Migrationen in Reihenfolge; der Index+1 wird in `PRAGMA user_version` abgelegt.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_stammdaten.sql"),
    include_str!("../migrations/0002_bestellungen.sql"),
    include_str!("../migrations/0003_finanzen.sql"),
    include_str!("../migrations/0004_lager_anlagen.sql"),
    include_str!("../migrations/0005_produktion.sql"),
    include_str!("../migrations/0006_dashboard.sql"),
];

pub struct Db {
    pub(crate) conn: Connection,
    /// Ordner mit Datenbank, `rechnungen/`, später `belege/` usw.
    data_dir: PathBuf,
    /// Datenbankdatei (nicht bei der Speicher-Datenbank der Tests)
    db_path: Option<PathBuf>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        std::fs::create_dir_all(&dir)?;
        Self::init(Connection::open(path)?, dir, Some(path.to_path_buf()))
    }

    /// Für Tests: Datenbank im Speicher, Dateien in einem eigenen Temp-Ordner.
    pub fn open_in_memory() -> Result<Db> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "druckplaner_mem_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        Self::init(Connection::open_in_memory()?, dir, None)
    }

    fn init(conn: Connection, data_dir: PathBuf, db_path: Option<PathBuf>) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let mut db = Db { conn, data_dir, db_path };
        db.migrate()?;
        Ok(db)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Pfad der Datenbankdatei, falls es eine gibt.
    pub fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }

    fn migrate(&mut self) -> Result<()> {
        let current: usize = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_sets_version_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("druckplaner_test_{}", std::process::id()));
        let path = dir.join("test.sqlite");
        {
            Db::open(&path).unwrap();
        }
        let db = Db::open(&path).unwrap();
        let v: usize = db
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, MIGRATIONS.len());
        assert_eq!(db.data_dir(), dir);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
