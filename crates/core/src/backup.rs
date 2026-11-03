//! Datensicherung: ein ZIP mit einem konsistenten Abzug der Datenbank (`VACUUM INTO`) und allen Dateien
//! (Belege, Rechnungen, .3mf-Dateien, eigene Vorlagen). Automatisch beim Beenden, wenn sich etwas geändert hat; die
//! letzten N Sicherungen bleiben erhalten.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    db::Db,
    error::{Error, Result},
};

/// Ordner im Datenordner, die mitgesichert werden.
const BACKED_UP_DIRS: [&str; 4] = ["belege", "rechnungen", "3mf", "vorlagen"];
const DB_ENTRY: &str = "druckplaner.sqlite";
const PREFIX: &str = "druckplaner_backup_";

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
}

impl BackupInfo {
    /// Zeitpunkt aus dem Dateinamen, z. B. `2026-10-25 14:30`.
    pub fn label(&self) -> String {
        let stamp = self.file_name.trim_start_matches(PREFIX).trim_end_matches(".zip");
        match stamp.split_once('_') {
            Some((day, time)) if time.len() == 4 => format!("{day} {}:{}", &time[..2], &time[2..]),
            _ => stamp.to_string(),
        }
    }
}

fn sorted_dir(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default();
    entries.sort();
    entries
}

/// Fügt eine Datei unter dem Namen `name` (mit `/` getrennt) hinzu.
fn add_file(zip: &mut zip::ZipWriter<File>, name: &str, path: &Path) -> Result<()> {
    let bad = |e: zip::result::ZipError| Error::Invalid(format!("Sicherung konnte nicht geschrieben werden: {e}"));
    zip.start_file(name, zip::write::SimpleFileOptions::default()).map_err(bad)?;
    std::io::copy(&mut File::open(path)?, zip)?;
    Ok(())
}

fn add_dir(zip: &mut zip::ZipWriter<File>, rel: &str, dir: &Path, count: &mut usize) -> Result<()> {
    for path in sorted_dir(dir) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let entry = format!("{rel}/{name}");
        if path.is_dir() {
            add_dir(zip, &entry, &path, count)?;
        } else if path.is_file() {
            add_file(zip, &entry, &path)?;
            *count += 1;
        }
    }
    Ok(())
}

impl Db {
    /// Zielordner der Sicherungen: Einstellung oder `<Datenordner>/backups`.
    pub fn backup_dir(&self) -> Result<PathBuf> {
        let configured = self.setting_or_default("backup_dir")?;
        let configured = configured.trim();
        Ok(if configured.is_empty() { self.data_dir().join("backups") } else { PathBuf::from(configured) })
    }

    fn backup_keep(&self) -> usize {
        self.setting_or_default("backup_keep").ok().and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n >= 1).unwrap_or(10)
    }

    /// Vorhandene Sicherungen, neueste zuerst.
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let mut backups: Vec<BackupInfo> = sorted_dir(&self.backup_dir()?)
            .into_iter()
            .filter_map(|path| {
                let file_name = path.file_name()?.to_str()?.to_string();
                (file_name.starts_with(PREFIX) && file_name.ends_with(".zip")).then(|| BackupInfo {
                    size_bytes: fs::metadata(&path).map_or(0, |m| m.len()),
                    path,
                    file_name,
                })
            })
            .collect();
        backups.reverse();
        Ok(backups)
    }

    /// Erstellt jetzt eine Sicherung und räumt alte auf. Gibt den Pfad der ZIP-Datei zurück.
    pub fn create_backup(&self) -> Result<PathBuf> {
        let stamp = self.now()?.replace(' ', "_").replace(':', "");
        self.create_backup_named(&stamp)
    }

    fn create_backup_named(&self, stamp: &str) -> Result<PathBuf> {
        let dir = self.backup_dir()?;
        fs::create_dir_all(&dir)?;
        let target = dir.join(format!("{PREFIX}{stamp}.zip"));
        let partial = dir.join(format!("{PREFIX}{stamp}.zip.tmp"));
        let snapshot = dir.join(format!(".snapshot_{}.sqlite", std::process::id()));
        let _ = fs::remove_file(&snapshot);

        // konsistenter Abzug der laufenden Datenbank
        self.conn.execute_batch(&format!("VACUUM INTO '{}'", snapshot.to_string_lossy().replace('\'', "''")))?;
        let result = (|| -> Result<()> {
            let mut zip = zip::ZipWriter::new(File::create(&partial)?);
            add_file(&mut zip, DB_ENTRY, &snapshot)?;
            let mut files = 0;
            for name in BACKED_UP_DIRS {
                let folder = self.data_dir().join(name);
                if folder.is_dir() {
                    add_dir(&mut zip, name, &folder, &mut files)?;
                }
            }
            zip.finish().map_err(|e| Error::Invalid(format!("Sicherung konnte nicht abgeschlossen werden: {e}")))?.flush()?;
            Ok(())
        })();
        let _ = fs::remove_file(&snapshot);
        if let Err(e) = result {
            let _ = fs::remove_file(&partial);
            return Err(e);
        }
        if target.exists() {
            fs::remove_file(&target)?;
        }
        fs::rename(&partial, &target)?;
        self.prune_backups()?;
        Ok(target)
    }

    fn prune_backups(&self) -> Result<()> {
        for old in self.list_backups()?.into_iter().skip(self.backup_keep()) {
            fs::remove_file(old.path)?;
        }
        Ok(())
    }

    /// Automatische Sicherung beim Beenden: nur wenn eingeschaltet und die Datenbank neuer ist als die letzte Sicherung.
    pub fn auto_backup_if_needed(&self) -> Result<Option<PathBuf>> {
        if self.setting_or_default("backup_auto")?.trim().eq_ignore_ascii_case("nein") {
            return Ok(None);
        }
        let Some(db_path) = self.db_path() else { return Ok(None) };
        let db_changed = fs::metadata(db_path)?.modified()?;
        if let Some(latest) = self.list_backups()?.first() {
            if fs::metadata(&latest.path)?.modified()? >= db_changed {
                return Ok(None);
            }
        }
        self.create_backup().map(Some)
    }
}

/// Entpackt eine Sicherung in einen neuen oder leeren Ordner (überschreibt nie etwas) und prüft, dass sich die
/// Datenbank öffnen lässt. Gibt die Zahl der entpackten Dateien zurück.
pub fn restore_backup(zip_path: &Path, dest: &Path) -> Result<usize> {
    if dest.exists() && fs::read_dir(dest)?.next().is_some() {
        return Err(Error::Invalid("Der Zielordner ist nicht leer – bitte einen neuen oder leeren Ordner wählen".into()));
    }
    let bad = |e: &dyn std::fmt::Display| Error::Invalid(format!("Sicherung konnte nicht gelesen werden: {e}"));
    let mut archive = zip::ZipArchive::new(File::open(zip_path)?).map_err(|e| bad(&e))?;
    fs::create_dir_all(dest)?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| bad(&e))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(Error::Invalid("Die Sicherung enthält einen unzulässigen Pfad".into()));
        };
        let out = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut File::create(&out)?)?;
        count += 1;
    }
    let db_file = dest.join(DB_ENTRY);
    if !db_file.is_file() {
        return Err(Error::Invalid("In der Sicherung fehlt die Datenbank (druckplaner.sqlite)".into()));
    }
    Db::open(&db_file)?; // prüft, dass sie sich öffnen (und aktualisieren) lässt
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Customer;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("druckplaner_backup_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn open(dir: &Path) -> Db {
        Db::open(&dir.join(DB_ENTRY)).unwrap()
    }

    #[test]
    fn backup_roundtrip_restores_database_and_files() {
        let root = temp_dir("roundtrip");
        let live = root.join("live");
        let db = open(&live);
        let mut c = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut c).unwrap();
        fs::create_dir_all(live.join("belege/2026")).unwrap();
        fs::write(live.join("belege/2026/beleg.pdf"), b"%PDF-beleg").unwrap();
        fs::create_dir_all(live.join("rechnungen/2026")).unwrap();
        fs::write(live.join("rechnungen/2026/Rechnung_2026_1001.pdf"), b"%PDF-rechnung").unwrap();
        fs::create_dir_all(live.join("exporte")).unwrap();
        fs::write(live.join("exporte/EUER_2026.csv"), b"nicht sichern").unwrap();

        let zip = db.create_backup().unwrap();
        assert!(zip.starts_with(live.join("backups")), "Standard: <Datenordner>/backups");
        assert!(zip.is_file());
        let listed = db.list_backups().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].size_bytes > 0 && listed[0].label().len() == 16, "{}", listed[0].label());
        assert!(!live.join("backups").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with('.')), "Abzug wird aufgeräumt");

        // Änderung nach der Sicherung ist in der Sicherung nicht enthalten
        let mut later = Customer { company: "Später".into(), ..Default::default() };
        db.save_customer(&mut later).unwrap();

        let restored = root.join("wiederhergestellt");
        assert_eq!(restore_backup(&zip, &restored).unwrap(), 3, "Datenbank + Beleg + Rechnung, nicht der Export");
        let db2 = open(&restored);
        assert_eq!(db2.list_customers().unwrap().len(), 1);
        assert_eq!(fs::read(restored.join("belege/2026/beleg.pdf")).unwrap(), b"%PDF-beleg");
        assert!(restored.join("rechnungen/2026/Rechnung_2026_1001.pdf").is_file());
        assert!(!restored.join("exporte").exists());

        // nie in einen belegten Ordner entpacken
        assert!(restore_backup(&zip, &restored).is_err());
        assert!(restore_backup(&root.join("gibt-es-nicht.zip"), &root.join("neu")).is_err());
        drop((db, db2));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_the_newest_backups_are_kept_and_the_target_folder_is_configurable() {
        let root = temp_dir("prune");
        let db = open(&root.join("live"));
        let elsewhere = root.join("usb-stick");
        db.set_setting("backup_dir", &elsewhere.to_string_lossy()).unwrap();
        db.set_setting("backup_keep", "2").unwrap();
        for stamp in ["2026-10-01_0800", "2026-10-02_0800", "2026-10-03_0800", "2026-10-04_0800"] {
            db.create_backup_named(stamp).unwrap();
        }
        let names: Vec<String> = db.list_backups().unwrap().into_iter().map(|b| b.file_name).collect();
        assert_eq!(names, vec!["druckplaner_backup_2026-10-04_0800.zip", "druckplaner_backup_2026-10-03_0800.zip"]);
        assert_eq!(db.list_backups().unwrap()[0].label(), "2026-10-04 08:00");
        assert!(!root.join("live/backups").exists());

        db.set_setting("backup_keep", "kaputt").unwrap();
        assert_eq!(db.backup_keep(), 10, "ungültiger Wert → Standard");
        drop(db);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn automatic_backup_runs_only_when_something_changed() {
        let root = temp_dir("auto");
        let db = open(&root.join("live"));
        db.set_setting("owner_name", "Test").unwrap();
        assert!(db.auto_backup_if_needed().unwrap().is_some(), "noch keine Sicherung");
        assert!(db.auto_backup_if_needed().unwrap().is_none(), "nichts geändert");

        std::thread::sleep(std::time::Duration::from_millis(30));
        db.set_setting("owner_name", "Anders").unwrap();
        assert!(db.auto_backup_if_needed().unwrap().is_some(), "Datenbank ist neuer");

        db.set_setting("backup_auto", "nein").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        db.set_setting("owner_name", "Nochmal").unwrap();
        assert!(db.auto_backup_if_needed().unwrap().is_none(), "ausgeschaltet");

        assert!(Db::open_in_memory().unwrap().auto_backup_if_needed().unwrap().is_none(), "Speicher-Datenbank hat keine Datei");
        drop(db);
        let _ = fs::remove_dir_all(root);
    }
}
