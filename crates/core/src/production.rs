//! Produktionsplanung für einen Drucker: Plattenvorlagen, Bedarf aus offenen Bestellungen und
//! Mindestbeständen, Jobvorschläge, Warteschlange (Druckjobs) und Zeitplan.
//!
//! Ein Druckjob druckt eine Platte (alle Teile einer Plattenvorlage) in einer Farbe. Beim Abschluss bucht
//! das Programm Filament ab und die fertigen Teile ins Lager; bei einem Fehldruck nur das Filament.

use std::{collections::HashMap, path::Path};

use rusqlite::params;

use crate::{
    date,
    db::Db,
    error::{map_constraint, Error, Result},
    stock::StockKey,
};

// ------------------------------------------------------- Plattenvorlagen

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TemplatePart {
    pub print_part_id: i64,
    pub qty: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlateTemplate {
    pub id: i64,
    pub name: String,
    /// Werkstoff, für den geslict wurde; leer = beliebig
    pub material_kind: String,
    pub print_seconds: i64,
    pub filament_g: f64,
    /// relativ zum Datenordner
    pub file_3mf: String,
    pub note: String,
    pub parts: Vec<TemplatePart>,
}

impl PlateTemplate {
    pub fn minutes(&self) -> i64 {
        (self.print_seconds + 59) / 60
    }

    pub fn duration_label(&self) -> String {
        let m = self.minutes();
        format!("{} h {:02} min", m / 60, m % 60)
    }

    pub fn qty_of(&self, print_part_id: i64) -> i64 {
        self.parts.iter().find(|p| p.print_part_id == print_part_id).map_or(0, |p| p.qty)
    }
}

// ------------------------------------------------------------ Druckjobs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Planned,
    Printing,
    Done,
    Failed,
}

impl JobStatus {
    pub fn label(self) -> &'static str {
        match self {
            JobStatus::Planned => "Geplant",
            JobStatus::Printing => "Druckt",
            JobStatus::Done => "Fertig",
            JobStatus::Failed => "Fehldruck",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "printing" => JobStatus::Printing,
            "done" => JobStatus::Done,
            "failed" => JobStatus::Failed,
            _ => JobStatus::Planned,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrintJob {
    pub id: i64,
    pub template_id: i64,
    pub material_id: i64,
    pub status: JobStatus,
    pub position: i64,
    pub started_at: String,
    pub finished_at: String,
    pub used_g: i64,
    pub note: String,
}

// ---------------------------------------------------------------- Bedarf

/// Bedarf an fertigen Teilen einer Farbe.
#[derive(Debug, Clone)]
pub struct NeedRow {
    pub print_part_id: i64,
    pub material_id: i64,
    pub part_name: String,
    pub material_label: String,
    /// Menge, die offene Bestellungen noch brauchen (noch nicht versendet)
    pub demand: i64,
    /// Gewünschter Mindestbestand
    pub min_target: i64,
    pub stock: i64,
    /// Menge aus geplanten und laufenden Druckjobs
    pub planned: i64,
    /// Frühester Wunschtermin der betroffenen Bestellungen (leer = keiner)
    pub earliest_due: String,
}

impl NeedRow {
    /// Noch zu druckende Menge: Bedarf plus Mindestbestand minus Lager und Geplantes.
    pub fn missing(&self) -> i64 {
        (self.demand + self.min_target - self.stock - self.planned).max(0)
    }

    pub fn key(&self) -> StockKey {
        StockKey::PrintPart { print_part_id: self.print_part_id, material_id: self.material_id }
    }
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub template_id: i64,
    pub template_name: String,
    pub material_id: i64,
    pub material_label: String,
    /// Anzahl Platten
    pub count: i64,
    /// Erzeugte Teile insgesamt: (Teilname, Menge)
    pub produces: Vec<(String, i64)>,
    pub minutes_total: i64,
    pub filament_g_total: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Suggestions {
    pub items: Vec<Suggestion>,
    /// Bedarf, für den es keine passende Plattenvorlage gibt
    pub unmet: Vec<String>,
}

// -------------------------------------------------------------- Zeitplan

#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub job: PrintJob,
    pub template_name: String,
    pub material_label: String,
    /// `JJJJ-MM-TT HH:MM`
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone)]
pub struct FilamentNeed {
    pub material_label: String,
    pub need_g: i64,
    pub stock_g: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Schedule {
    pub jobs: Vec<ScheduledJob>,
    /// Ende des letzten Jobs (bzw. jetzt, wenn die Warteschlange leer ist)
    pub ends_at: String,
    /// Filamentbedarf der Warteschlange je Material gegenüber dem Lagerbestand
    pub filament: Vec<FilamentNeed>,
}

/// Frühester Startzeitpunkt ab `minute`, zu dem die Platte gewechselt werden kann (Fenster `from..=to`).
fn within_window(minute: i64, from: i64, to: i64) -> i64 {
    let time_of_day = minute.rem_euclid(1440);
    let day_start = minute - time_of_day;
    if time_of_day < from {
        day_start + from
    } else if time_of_day > to {
        day_start + 1440 + from
    } else {
        minute
    }
}

fn collect<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    Ok(rows.collect::<rusqlite::Result<Vec<T>>>()?)
}

fn job_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PrintJob> {
    let status: String = r.get(3)?;
    Ok(PrintJob {
        id: r.get(0)?,
        template_id: r.get(1)?,
        material_id: r.get(2)?,
        status: JobStatus::from_key(&status),
        position: r.get(4)?,
        started_at: r.get(5)?,
        finished_at: r.get(6)?,
        used_g: r.get(7)?,
        note: r.get(8)?,
    })
}

const JOB_SELECT: &str =
    "SELECT id, plate_template_id, material_id, status, position, started_at, finished_at, used_g, note FROM print_job";

impl Db {
    // ---- Plattenvorlagen

    pub fn list_plate_templates(&self) -> Result<Vec<PlateTemplate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, material_kind, print_seconds, filament_g, file_3mf, note
             FROM plate_template ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PlateTemplate {
                id: r.get(0)?,
                name: r.get(1)?,
                material_kind: r.get(2)?,
                print_seconds: r.get(3)?,
                filament_g: r.get(4)?,
                file_3mf: r.get(5)?,
                note: r.get(6)?,
                parts: Vec::new(),
            })
        })?;
        let mut templates = collect(rows)?;
        let mut part_stmt = self.conn.prepare(
            "SELECT print_part_id, qty FROM plate_template_part WHERE plate_template_id = ?1 ORDER BY print_part_id",
        )?;
        for t in &mut templates {
            let parts = part_stmt.query_map([t.id], |r| Ok(TemplatePart { print_part_id: r.get(0)?, qty: r.get(1)? }))?;
            t.parts = collect(parts)?;
        }
        Ok(templates)
    }

    pub fn save_plate_template(&self, t: &mut PlateTemplate) -> Result<()> {
        t.name = t.name.trim().to_string();
        if t.name.is_empty() {
            return Err(Error::Invalid("Name fehlt".into()));
        }
        if t.print_seconds <= 0 {
            return Err(Error::Invalid("Druckzeit fehlt".into()));
        }
        if t.filament_g < 0.0 || !t.filament_g.is_finite() {
            return Err(Error::Invalid("Filamentverbrauch ist ungültig".into()));
        }
        if t.parts.is_empty() {
            return Err(Error::Invalid("Die Platte enthält keine Teile".into()));
        }
        if t.parts.iter().any(|p| p.qty <= 0 || p.print_part_id == 0) {
            return Err(Error::Invalid("Jedes Teil braucht eine Menge größer als 0".into()));
        }
        let tx = self.conn.unchecked_transaction()?;
        let dup = "Eine Plattenvorlage mit diesem Namen existiert bereits";
        let id = if t.id == 0 {
            tx.execute(
                "INSERT INTO plate_template (name, material_kind, print_seconds, filament_g, file_3mf, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![t.name, t.material_kind.trim(), t.print_seconds, t.filament_g, t.file_3mf, t.note],
            )
            .map_err(|e| map_constraint(e, dup))?;
            tx.last_insert_rowid()
        } else {
            tx.execute(
                "UPDATE plate_template SET name = ?1, material_kind = ?2, print_seconds = ?3, filament_g = ?4,
                 file_3mf = ?5, note = ?6 WHERE id = ?7",
                params![t.name, t.material_kind.trim(), t.print_seconds, t.filament_g, t.file_3mf, t.note, t.id],
            )
            .map_err(|e| map_constraint(e, dup))?;
            t.id
        };
        tx.execute("DELETE FROM plate_template_part WHERE plate_template_id = ?1", [id])?;
        for p in &t.parts {
            tx.execute(
                "INSERT INTO plate_template_part (plate_template_id, print_part_id, qty) VALUES (?1, ?2, ?3)",
                params![id, p.print_part_id, p.qty],
            )
            .map_err(|e| map_constraint(e, "Ein Druckteil ist doppelt auf der Platte eingetragen"))?;
        }
        tx.commit()?;
        t.id = id;
        Ok(())
    }

    pub fn delete_plate_template(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM plate_template WHERE id = ?1", [id])
            .map_err(|e| map_constraint(e, "Die Plattenvorlage wird von Druckjobs verwendet"))?;
        Ok(())
    }

    /// Legt eine Kopie der .3mf-Datei unter `3mf/` ab und verknüpft sie mit der Vorlage.
    pub fn attach_3mf(&self, template_id: i64, source: &Path) -> Result<String> {
        if !source.is_file() {
            return Err(Error::Invalid(format!("Datei nicht gefunden: {}", source.display())));
        }
        let name: String =
            self.conn.query_row("SELECT name FROM plate_template WHERE id = ?1", [template_id], |r| r.get(0))?;
        let safe: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        let rel = format!("3mf/{template_id}_{safe}.3mf");
        let target = rel.split('/').fold(self.data_dir().to_path_buf(), |dir, part| dir.join(part));
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(source, &target)?;
        self.conn.execute("UPDATE plate_template SET file_3mf = ?1 WHERE id = ?2", params![rel, template_id])?;
        Ok(rel)
    }

    // ---- Druckjobs

    /// Warteschlange (laufender Druck zuerst, dann geplante nach Position) oder die zuletzt abgeschlossenen Jobs.
    pub fn list_jobs(&self, closed: bool) -> Result<Vec<PrintJob>> {
        let sql = if closed {
            format!("{JOB_SELECT} WHERE status IN ('done', 'failed') ORDER BY finished_at DESC, id DESC LIMIT 30")
        } else {
            format!("{JOB_SELECT} WHERE status IN ('planned', 'printing') ORDER BY status = 'printing' DESC, position, id")
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], job_from_row)?;
        collect(rows)
    }

    fn load_job(&self, id: i64) -> Result<PrintJob> {
        let sql = format!("{JOB_SELECT} WHERE id = ?1");
        Ok(self.conn.query_row(&sql, [id], job_from_row)?)
    }

    fn template_by_id(&self, id: i64) -> Result<PlateTemplate> {
        self.list_plate_templates()?
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| Error::Invalid("Plattenvorlage nicht gefunden".into()))
    }

    /// Reiht einen neuen Job am Ende der Warteschlange ein.
    pub fn add_job(&self, template_id: i64, material_id: i64) -> Result<i64> {
        let template = self.template_by_id(template_id)?;
        let (kind, color): (String, String) = self
            .conn
            .query_row("SELECT kind, color FROM material WHERE id = ?1", [material_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| Error::Invalid("Material fehlt".into()))?;
        if !template.material_kind.is_empty() && template.material_kind != kind {
            return Err(Error::Invalid(format!(
                "Die Platte „{}“ ist für {} geslict, nicht für {kind} {color}",
                template.name, template.material_kind
            )));
        }
        let position: i64 =
            self.conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM print_job", [], |r| r.get(0))?;
        self.conn.execute(
            "INSERT INTO print_job (plate_template_id, material_id, position) VALUES (?1, ?2, ?3)",
            params![template_id, material_id, position],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Verschiebt einen geplanten Job um eine Stelle nach vorn (`up`) oder hinten.
    pub fn move_job(&self, id: i64, up: bool) -> Result<()> {
        let planned: Vec<PrintJob> = self.list_jobs(false)?.into_iter().filter(|j| j.status == JobStatus::Planned).collect();
        let Some(i) = planned.iter().position(|j| j.id == id) else { return Ok(()) };
        let other = if up { i.checked_sub(1) } else { (i + 1 < planned.len()).then_some(i + 1) };
        let Some(o) = other else { return Ok(()) };
        let (a, b) = (&planned[i], &planned[o]);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE print_job SET position = ?1 WHERE id = ?2", params![b.position, a.id])?;
        tx.execute("UPDATE print_job SET position = ?1 WHERE id = ?2", params![a.position, b.id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_job(&self, id: i64) -> Result<()> {
        if self.load_job(id)?.status != JobStatus::Planned {
            return Err(Error::Invalid("Nur geplante Jobs lassen sich entfernen".into()));
        }
        self.conn.execute("DELETE FROM print_job WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn start_job(&self, id: i64) -> Result<()> {
        let job = self.load_job(id)?;
        if job.status != JobStatus::Planned {
            return Err(Error::Invalid("Der Job ist nicht mehr geplant".into()));
        }
        let running: i64 =
            self.conn.query_row("SELECT COUNT(*) FROM print_job WHERE status = 'printing'", [], |r| r.get(0))?;
        if running > 0 {
            return Err(Error::Invalid("Es läuft bereits ein Druck – erst diesen abschließen".into()));
        }
        self.conn.execute(
            "UPDATE print_job SET status = 'printing', started_at = ?1 WHERE id = ?2",
            params![self.now()?, id],
        )?;
        Ok(())
    }

    /// Bucht Filament ab und – bei Erfolg – die gedruckten Teile ins Lager.
    fn book_job_stock(&self, job: &PrintJob, template: &PlateTemplate, used_g: i64, with_parts: bool, day: &str) -> Result<()> {
        if used_g > 0 {
            self.add_stock_move(
                StockKey::Filament { material_id: job.material_id },
                -used_g,
                day,
                "production",
                ("print_job", job.id),
                &template.name,
            )?;
        }
        if with_parts {
            for p in &template.parts {
                self.add_stock_move(
                    StockKey::PrintPart { print_part_id: p.print_part_id, material_id: job.material_id },
                    p.qty,
                    day,
                    "production",
                    ("print_job", job.id),
                    &template.name,
                )?;
            }
        }
        Ok(())
    }

    /// Schließt den Job erfolgreich ab: Filament (laut Vorlage) wird abgebucht, die Teile kommen ins Lager.
    pub fn finish_job(&self, id: i64) -> Result<()> {
        let job = self.load_job(id)?;
        if !matches!(job.status, JobStatus::Planned | JobStatus::Printing) {
            return Err(Error::Invalid("Der Job ist bereits abgeschlossen".into()));
        }
        let template = self.template_by_id(job.template_id)?;
        let now = self.now()?;
        let used = template.filament_g.round() as i64;
        let tx = self.conn.unchecked_transaction()?;
        let started = if job.started_at.is_empty() { now.clone() } else { job.started_at.clone() };
        tx.execute(
            "UPDATE print_job SET status = 'done', started_at = ?1, finished_at = ?2, used_g = ?3 WHERE id = ?4",
            params![started, now, used, id],
        )?;
        self.book_job_stock(&job, &template, used, true, &now[..10])?;
        tx.commit()?;
        Ok(())
    }

    /// Markiert den Job als Fehldruck: verbrauchtes Filament wird abgebucht, es entstehen keine Teile.
    pub fn fail_job(&self, id: i64, used_g: i64) -> Result<()> {
        let job = self.load_job(id)?;
        if !matches!(job.status, JobStatus::Planned | JobStatus::Printing) {
            return Err(Error::Invalid("Der Job ist bereits abgeschlossen".into()));
        }
        if used_g < 0 {
            return Err(Error::Invalid("Verbrauch darf nicht negativ sein".into()));
        }
        let template = self.template_by_id(job.template_id)?;
        let now = self.now()?;
        let tx = self.conn.unchecked_transaction()?;
        let started = if job.started_at.is_empty() { now.clone() } else { job.started_at.clone() };
        tx.execute(
            "UPDATE print_job SET status = 'failed', started_at = ?1, finished_at = ?2, used_g = ?3 WHERE id = ?4",
            params![started, now, used_g, id],
        )?;
        self.book_job_stock(&job, &template, used_g, false, &now[..10])?;
        tx.commit()?;
        Ok(())
    }

    /// Nimmt einen abgeschlossenen Job zurück in die Warteschlange und macht die Lagerbuchungen rückgängig.
    pub fn reopen_job(&self, id: i64) -> Result<()> {
        let job = self.load_job(id)?;
        if !matches!(job.status, JobStatus::Done | JobStatus::Failed) {
            return Err(Error::Invalid("Der Job ist nicht abgeschlossen".into()));
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM stock_move WHERE ref_type = 'print_job' AND ref_id = ?1", [id])?;
        let position: i64 = tx.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM print_job", [], |r| r.get(0))?;
        tx.execute(
            "UPDATE print_job SET status = 'planned', started_at = '', finished_at = '', used_g = 0, position = ?1
             WHERE id = ?2",
            params![position, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ---- Bedarf

    /// Bedarf an fertigen Teilen je Farbe: offene Bestellungen plus Mindestbestand, abzüglich Lager und
    /// geplanter Druckjobs. Dringendste (frühester Wunschtermin) zuerst.
    pub fn production_needs(&self) -> Result<Vec<NeedRow>> {
        #[derive(Default)]
        struct Acc {
            demand: i64,
            due: String,
        }
        let mut acc: HashMap<(i64, i64), Acc> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT b.print_part_id, ol.material_id,
                        SUM((ol.qty - COALESCE(sh.q, 0)) * b.qty), COALESCE(MIN(NULLIF(o.due_date, '')), '')
                 FROM order_line ol
                 JOIN sales_order o ON o.id = ol.order_id
                 JOIN product_bom b ON b.product_id = ol.product_id AND b.print_part_id IS NOT NULL
                 LEFT JOIN (SELECT sl.order_line_id AS line_id, SUM(sl.qty) AS q
                            FROM shipment_line sl JOIN shipment s ON s.id = sl.shipment_id
                            WHERE s.shipped_at <> '' GROUP BY sl.order_line_id) sh ON sh.line_id = ol.id
                 WHERE ol.product_id IS NOT NULL AND ol.material_id IS NOT NULL AND ol.qty - COALESCE(sh.q, 0) > 0
                 GROUP BY b.print_part_id, ol.material_id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?), r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
            })?;
            for row in rows {
                let (key, demand, due) = row?;
                let entry = acc.entry(key).or_default();
                entry.demand += demand;
                entry.due = due;
            }
        }
        let mins = self.finished_mins()?;
        for key in mins.keys() {
            if let StockKey::PrintPart { print_part_id, material_id } = key {
                acc.entry((*print_part_id, *material_id)).or_default();
            }
        }

        let mut planned: HashMap<(i64, i64), i64> = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT tp.print_part_id, j.material_id, SUM(tp.qty)
                 FROM print_job j JOIN plate_template_part tp ON tp.plate_template_id = j.plate_template_id
                 WHERE j.status IN ('planned', 'printing') GROUP BY tp.print_part_id, j.material_id",
            )?;
            let rows = stmt.query_map([], |r| Ok(((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?), r.get::<_, i64>(2)?)))?;
            for row in rows {
                let (key, qty) = row?;
                planned.insert(key, qty);
            }
        }

        let mut needs = Vec::new();
        for ((print_part_id, material_id), a) in acc {
            let key = StockKey::PrintPart { print_part_id, material_id };
            let min_target = mins.get(&key).copied().unwrap_or(0);
            if a.demand == 0 && min_target == 0 {
                continue;
            }
            let (label, _) = self.stock_label(key)?;
            let (part_name, material_label) = label.split_once(" – ").map_or((label.clone(), String::new()), |(p, m)| (p.to_string(), m.to_string()));
            needs.push(NeedRow {
                print_part_id,
                material_id,
                part_name,
                material_label,
                demand: a.demand,
                min_target,
                stock: self.stock_qty(key)?,
                planned: planned.get(&(print_part_id, material_id)).copied().unwrap_or(0),
                earliest_due: a.due,
            });
        }
        needs.sort_by(|a, b| {
            (a.earliest_due.is_empty(), &a.earliest_due, &a.part_name, &a.material_label)
                .cmp(&(b.earliest_due.is_empty(), &b.earliest_due, &b.part_name, &b.material_label))
        });
        Ok(needs)
    }

    /// Schlägt Druckjobs vor, die den fehlenden Bedarf decken: je Teil die Platte mit den meisten Exemplaren
    /// (bei Gleichstand die kürzeste); andere Teile auf derselben Platte werden mit angerechnet.
    pub fn suggest_jobs(&self) -> Result<Suggestions> {
        let needs: Vec<NeedRow> = self.production_needs()?.into_iter().filter(|n| n.missing() > 0).collect();
        let templates = self.list_plate_templates()?;
        let materials = self.list_materials()?;
        let part_names: HashMap<i64, String> = self.list_print_parts()?.into_iter().map(|p| (p.id, p.name)).collect();

        let mut remaining: HashMap<(i64, i64), i64> =
            needs.iter().map(|n| ((n.print_part_id, n.material_id), n.missing())).collect();
        let mut counts: Vec<((i64, i64), i64)> = Vec::new();
        let mut out = Suggestions::default();

        for need in &needs {
            let material = materials.iter().find(|m| m.id == need.material_id);
            let kind = material.map_or("", |m| m.kind.as_str());
            let mut guard = 0;
            while remaining.get(&(need.print_part_id, need.material_id)).copied().unwrap_or(0) > 0 && guard < 1000 {
                guard += 1;
                let best = templates
                    .iter()
                    .filter(|t| t.qty_of(need.print_part_id) > 0 && (t.material_kind.is_empty() || t.material_kind == kind))
                    .max_by_key(|t| (t.qty_of(need.print_part_id), -t.print_seconds));
                let Some(template) = best else {
                    out.unmet.push(format!(
                        "{} ({}): {} Stück fehlen, aber keine passende Plattenvorlage",
                        need.part_name,
                        need.material_label,
                        remaining[&(need.print_part_id, need.material_id)]
                    ));
                    remaining.insert((need.print_part_id, need.material_id), 0);
                    break;
                };
                match counts.iter_mut().find(|(k, _)| *k == (template.id, need.material_id)) {
                    Some((_, n)) => *n += 1,
                    None => counts.push(((template.id, need.material_id), 1)),
                }
                for p in &template.parts {
                    if let Some(r) = remaining.get_mut(&(p.print_part_id, need.material_id)) {
                        *r = (*r - p.qty).max(0);
                    }
                }
            }
        }

        for ((template_id, material_id), count) in counts {
            let Some(template) = templates.iter().find(|t| t.id == template_id) else { continue };
            out.items.push(Suggestion {
                template_id,
                template_name: template.name.clone(),
                material_id,
                material_label: materials.iter().find(|m| m.id == material_id).map_or(String::new(), |m| m.label()),
                count,
                produces: template
                    .parts
                    .iter()
                    .map(|p| (part_names.get(&p.print_part_id).cloned().unwrap_or_default(), p.qty * count))
                    .collect(),
                minutes_total: template.minutes() * count,
                filament_g_total: template.filament_g * count as f64,
            });
        }
        Ok(out)
    }

    /// Legt die vorgeschlagenen Platten als Jobs an. Gibt die Anzahl neuer Jobs zurück.
    pub fn apply_suggestion(&self, s: &Suggestion) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        for _ in 0..s.count {
            self.add_job(s.template_id, s.material_id)?;
        }
        tx.commit()?;
        Ok(s.count)
    }

    // ---- Zeitplan

    /// Berechnet Start und Ende aller Jobs der Warteschlange nacheinander auf dem einen Drucker. Neue Drucke
    /// beginnen nur innerhalb des Zeitfensters, in dem du die Platte wechseln kannst (Einstellungen).
    pub fn schedule(&self) -> Result<Schedule> {
        let now = self.now()?;
        let now_min = date::datetime_to_minutes(&now).unwrap_or(0);
        let (from, to) = match (
            date::parse_hhmm(&self.setting_or_default("plate_window_from")?),
            date::parse_hhmm(&self.setting_or_default("plate_window_to")?),
        ) {
            (Some(f), Some(t)) if f < t => (f, t),
            _ => (7 * 60, 23 * 60),
        };
        let templates = self.list_plate_templates()?;
        let materials = self.list_materials()?;

        let mut jobs = Vec::new();
        let mut available = now_min;
        let mut need_g: HashMap<i64, i64> = HashMap::new();
        for job in self.list_jobs(false)? {
            let Some(template) = templates.iter().find(|t| t.id == job.template_id) else { continue };
            let duration = template.minutes();
            let (start, end) = match (job.status, date::datetime_to_minutes(&job.started_at)) {
                (JobStatus::Printing, Some(started)) => {
                    let end = started + duration;
                    available = available.max(end);
                    (started, end)
                }
                _ => {
                    let start = within_window(available, from, to);
                    available = start + duration;
                    (start, start + duration)
                }
            };
            *need_g.entry(job.material_id).or_default() += template.filament_g.round() as i64;
            jobs.push(ScheduledJob {
                template_name: template.name.clone(),
                material_label: materials.iter().find(|m| m.id == job.material_id).map_or(String::new(), |m| m.label()),
                start: date::minutes_to_datetime(start),
                end: date::minutes_to_datetime(end),
                job,
            });
        }

        let mut filament = Vec::new();
        for m in &materials {
            if let Some(&need) = need_g.get(&m.id) {
                filament.push(FilamentNeed {
                    material_label: m.label(),
                    need_g: need,
                    stock_g: self.stock_qty(StockKey::Filament { material_id: m.id })?,
                });
            }
        }
        let ends_at = date::minutes_to_datetime(available);
        Ok(Schedule { jobs, ends_at, filament })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::{BomItem, BomLine, Customer, Material, PrintPart, Product},
        orders::{Order, OrderLine},
    };

    struct Setup {
        db: Db,
        material: Material,
        housing: PrintPart,
        sled: PrintPart,
        plate_a: PlateTemplate,
        plate_b: PlateTemplate,
        order: Order,
    }

    fn setup() -> Setup {
        let db = Db::open_in_memory().unwrap();
        let mut material = Material { kind: "PETG".into(), color: "black".into(), ..Default::default() };
        db.save_material(&mut material).unwrap();
        let mut housing = PrintPart { name: "Housing".into(), ..Default::default() };
        let mut sled = PrintPart { name: "Sled".into(), ..Default::default() };
        db.save_print_part(&mut housing).unwrap();
        db.save_print_part(&mut sled).unwrap();
        let mut product = Product {
            name: "Case Set".into(),
            bom: vec![
                BomLine { item: BomItem::PrintPart(housing.id), qty: 1 },
                BomLine { item: BomItem::PrintPart(sled.id), qty: 1 },
            ],
            ..Default::default()
        };
        db.save_product(&mut product).unwrap();
        let mut customer = Customer { company: "BluePulse".into(), ..Default::default() };
        db.save_customer(&mut customer).unwrap();
        let mut order = Order {
            customer_id: customer.id,
            received_at: "2026-10-01".into(),
            due_date: "2026-12-01".into(),
            lines: vec![OrderLine {
                product_id: Some(product.id),
                material_id: Some(material.id),
                qty: 5,
                unit_price_cents: 1300,
                ..Default::default()
            }],
            ..Default::default()
        };
        db.save_order(&mut order).unwrap();
        let mut plate_a = PlateTemplate {
            name: "Housing x6".into(),
            material_kind: "PETG".into(),
            print_seconds: 3600,
            filament_g: 60.0,
            parts: vec![TemplatePart { print_part_id: housing.id, qty: 6 }],
            ..Default::default()
        };
        let mut plate_b = PlateTemplate {
            name: "Sled x8".into(),
            print_seconds: 1800,
            filament_g: 30.0,
            parts: vec![TemplatePart { print_part_id: sled.id, qty: 8 }],
            ..Default::default()
        };
        db.save_plate_template(&mut plate_a).unwrap();
        db.save_plate_template(&mut plate_b).unwrap();
        Setup { db, material, housing, sled, plate_a, plate_b, order }
    }

    #[test]
    fn template_validation_and_roundtrip() {
        let s = setup();
        let db = &s.db;
        assert!(db.save_plate_template(&mut PlateTemplate::default()).is_err());
        let mut no_parts = PlateTemplate { name: "Leer".into(), print_seconds: 60, ..Default::default() };
        assert!(db.save_plate_template(&mut no_parts).is_err());
        let mut dup = PlateTemplate { name: "Sled x8".into(), parts: s.plate_b.parts.clone(), print_seconds: 60, ..Default::default() };
        assert!(db.save_plate_template(&mut dup).is_err());

        let list = db.list_plate_templates().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].duration_label(), "1 h 00 min");
        assert_eq!(list[1].qty_of(s.sled.id), 8);

        let mut edited = list[0].clone();
        edited.parts.push(TemplatePart { print_part_id: s.sled.id, qty: 2 });
        db.save_plate_template(&mut edited).unwrap();
        assert_eq!(db.list_plate_templates().unwrap()[0].parts.len(), 2);
    }

    #[test]
    fn needs_come_from_open_orders_and_minimum_stock() {
        let s = setup();
        let db = &s.db;
        let housing = StockKey::PrintPart { print_part_id: s.housing.id, material_id: s.material.id };

        let needs = db.production_needs().unwrap();
        assert_eq!(needs.len(), 2);
        assert!(needs.iter().all(|n| n.demand == 5 && n.missing() == 5 && n.earliest_due == "2026-12-01"));

        db.set_finished_min(housing, 4).unwrap();
        db.stock_inventory(housing, 3, "2026-10-02", "").unwrap();
        let h = db.production_needs().unwrap().into_iter().find(|n| n.print_part_id == s.housing.id).unwrap();
        assert_eq!((h.demand, h.min_target, h.stock, h.planned, h.missing()), (5, 4, 3, 0, 6));

        // Minimum allein erzeugt Bedarf, auch ohne Bestellung
        let mut sled_min_only = db.production_needs().unwrap();
        sled_min_only.retain(|n| n.print_part_id == s.sled.id);
        assert_eq!(sled_min_only[0].missing(), 5);

        // nach Versand eines Teils sinkt der Bedarf
        let line = s.order.lines[0].id;
        let mut ship = db.new_shipment_draft(s.order.id).unwrap();
        ship.set_qty(line, 2);
        ship.shipped_at = "2026-10-05".into();
        db.save_shipment(&mut ship).unwrap();
        let sled = db.production_needs().unwrap().into_iter().find(|n| n.print_part_id == s.sled.id).unwrap();
        assert_eq!(sled.demand, 3);

        assert!(db.set_finished_min(StockKey::Filament { material_id: 1 }, 3).is_err());
        db.set_finished_min(housing, 0).unwrap();
        assert!(db.finished_mins().unwrap().is_empty());
    }

    #[test]
    fn suggestions_cover_the_gap_and_jobs_close_it() {
        let s = setup();
        let db = &s.db;
        let housing = StockKey::PrintPart { print_part_id: s.housing.id, material_id: s.material.id };
        db.set_finished_min(housing, 4).unwrap(); // Housing: 5 + 4 = 9 fehlen

        let sug = db.suggest_jobs().unwrap();
        assert!(sug.unmet.is_empty());
        assert_eq!(sug.items.len(), 2);
        let a = sug.items.iter().find(|i| i.template_id == s.plate_a.id).unwrap();
        assert_eq!((a.count, a.minutes_total, a.filament_g_total), (2, 120, 120.0), "2 Platten à 6 = 12 ≥ 9");
        assert_eq!(a.produces, vec![("Housing".to_string(), 12)]);
        let b = sug.items.iter().find(|i| i.template_id == s.plate_b.id).unwrap();
        assert_eq!(b.count, 1);

        for item in &sug.items {
            db.apply_suggestion(item).unwrap();
        }
        assert!(db.production_needs().unwrap().iter().all(|n| n.missing() == 0), "geplante Jobs decken den Bedarf");
        assert!(db.suggest_jobs().unwrap().items.is_empty());
        assert_eq!(db.list_jobs(false).unwrap().len(), 3);

        // ohne passende Vorlage: Hinweis statt Vorschlag
        db.delete_job(db.list_jobs(false).unwrap()[2].id).unwrap();
        for j in db.list_jobs(false).unwrap() {
            db.delete_job(j.id).unwrap();
        }
        db.delete_plate_template(s.plate_b.id).unwrap();
        let sug = db.suggest_jobs().unwrap();
        assert_eq!(sug.unmet.len(), 1);
        assert!(sug.unmet[0].starts_with("Sled"));
    }

    #[test]
    fn job_lifecycle_books_filament_and_parts() {
        let s = setup();
        let db = &s.db;
        let filament = StockKey::Filament { material_id: s.material.id };
        let housing = StockKey::PrintPart { print_part_id: s.housing.id, material_id: s.material.id };
        db.stock_inventory(filament, 1000, "2026-10-01", "").unwrap();

        let j1 = db.add_job(s.plate_a.id, s.material.id).unwrap();
        let j2 = db.add_job(s.plate_b.id, s.material.id).unwrap();
        let j3 = db.add_job(s.plate_a.id, s.material.id).unwrap();

        // Reihenfolge ändern
        db.move_job(j3, true).unwrap();
        let order: Vec<i64> = db.list_jobs(false).unwrap().iter().map(|j| j.id).collect();
        assert_eq!(order, vec![j1, j3, j2]);
        db.move_job(j1, true).unwrap(); // schon vorn: keine Änderung
        assert_eq!(db.list_jobs(false).unwrap()[0].id, j1);

        db.start_job(j1).unwrap();
        assert!(db.start_job(j3).is_err(), "nur ein Druck gleichzeitig");
        assert!(db.delete_job(j1).is_err(), "laufenden Job nicht löschen");
        assert_eq!(db.list_jobs(false).unwrap()[0].status, JobStatus::Printing);

        db.finish_job(j1).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(filament).unwrap()), (6, 940));
        assert!(db.finish_job(j1).is_err());

        db.fail_job(j3, 20).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(filament).unwrap()), (6, 920));
        assert_eq!(db.list_jobs(true).unwrap().len(), 2);

        // Zurücknehmen macht die Buchungen rückgängig
        db.reopen_job(j1).unwrap();
        assert_eq!((db.stock_qty(housing).unwrap(), db.stock_qty(filament).unwrap()), (0, 980));
        assert_eq!(db.list_jobs(false).unwrap().len(), 2);
        assert!(db.reopen_job(j2).is_err());

        // Werkstoff muss zur Vorlage passen
        let mut cf = Material { kind: "PETG-CF".into(), color: "black".into(), ..Default::default() };
        db.save_material(&mut cf).unwrap();
        assert!(db.add_job(s.plate_a.id, cf.id).is_err());
        assert!(db.add_job(s.plate_b.id, cf.id).is_ok(), "Vorlage ohne Werkstoff passt überall");
    }

    #[test]
    fn schedule_runs_jobs_back_to_back_inside_the_change_window() {
        let s = setup();
        let db = &s.db;
        db.add_job(s.plate_a.id, s.material.id).unwrap(); // 60 min
        db.add_job(s.plate_b.id, s.material.id).unwrap(); // 30 min
        db.add_job(s.plate_a.id, s.material.id).unwrap();

        let plan = db.schedule().unwrap();
        assert_eq!(plan.jobs.len(), 3);
        let now = date::datetime_to_minutes(&db.now().unwrap()).unwrap();
        let mut previous_end = now;
        for j in &plan.jobs {
            let start = date::datetime_to_minutes(&j.start).unwrap();
            let end = date::datetime_to_minutes(&j.end).unwrap();
            assert!(start >= previous_end, "Jobs überlappen nicht");
            assert!(start >= now);
            let time_of_day = start.rem_euclid(1440);
            assert!((7 * 60..=23 * 60).contains(&time_of_day), "Start im Wechselfenster: {}", j.start);
            previous_end = end;
        }
        assert_eq!(plan.ends_at, plan.jobs[2].end);
        assert_eq!(plan.filament.len(), 1);
        assert_eq!(plan.filament[0].need_g, 150);
        assert_eq!(plan.filament[0].stock_g, 0);
    }

    #[test]
    fn window_logic_moves_starts_to_the_next_opening() {
        let (from, to) = (7 * 60, 23 * 60);
        assert_eq!(within_window(10 * 60, from, to), 10 * 60);
        assert_eq!(within_window(3 * 60, from, to), 7 * 60, "zu früh → heute 07:00");
        assert_eq!(within_window(23 * 60 + 30, from, to), 1440 + 7 * 60, "zu spät → morgen 07:00");
        assert_eq!(within_window(1440 + 2 * 60, from, to), 1440 + 7 * 60);
        assert_eq!(within_window(to, from, to), to);
    }
}
