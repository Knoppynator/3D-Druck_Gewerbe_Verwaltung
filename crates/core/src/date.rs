//! Datumswerte werden als ISO-Text `JJJJ-MM-TT` gespeichert und deutsch (`TT.MM.JJJJ`) angezeigt.

use rusqlite::params;

use crate::{
    db::Db,
    error::{Error, Result},
};

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// `30.9.2026` oder `30.09.26` → `2026-09-30`.
pub fn parse_de(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let day: u32 = parts[0].trim().parse().ok()?;
    let month: u32 = parts[1].trim().parse().ok()?;
    let mut year: i32 = parts[2].trim().parse().ok()?;
    if year < 100 {
        year += 2000;
    }
    if !(1..=12).contains(&month) || !(1900..=2200).contains(&year) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// `2026-09-30` → `30.09.2026`; alles andere (auch leer) bleibt unverändert.
pub fn format_de(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    if p.len() == 3 && p[0].len() == 4 {
        format!("{}.{}.{}", p[2], p[1], p[0])
    } else {
        iso.to_string()
    }
}

pub fn is_valid_iso(iso: &str) -> bool {
    let p: Vec<&str> = iso.split('-').collect();
    p.len() == 3 && p[0].len() == 4 && parse_de(&format!("{}.{}.{}", p[2], p[1], p[0])).as_deref() == Some(iso)
}

pub fn year_of(iso: &str) -> Option<i32> {
    iso.get(0..4)?.parse().ok()
}

impl Db {
    /// Heutiges Datum (lokale Zeit) als ISO-Text.
    pub fn today(&self) -> Result<String> {
        Ok(self.conn.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0))?)
    }

    pub fn add_days(&self, iso: &str, days: i64) -> Result<String> {
        let shifted: Option<String> = self
            .conn
            .query_row("SELECT date(?1, ?2)", params![iso, format!("{days:+} days")], |r| r.get(0))?;
        shifted.ok_or_else(|| Error::Invalid(format!("Ungültiges Datum: {iso}")))
    }

    /// Anzahl Tage von `from` bis `to` (negativ, wenn `to` früher liegt).
    pub fn days_between(&self, from: &str, to: &str) -> Result<i64> {
        let days: Option<i64> = self.conn.query_row(
            "SELECT CAST(julianday(?2) - julianday(?1) AS INTEGER)",
            params![from, to],
            |r| r.get(0),
        )?;
        days.ok_or_else(|| Error::Invalid(format!("Ungültiges Datum: {from} / {to}")))
    }
}


// ------------------------------------------------- Datum mit Uhrzeit (Druckplanung)

/// Tage seit 1970-01-01 für ein Datum im proleptischen gregorianischen Kalender.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// `JJJJ-MM-TT HH:MM` → Minuten seit 1970-01-01 00:00.
pub fn datetime_to_minutes(s: &str) -> Option<i64> {
    let (day, time) = s.split_once(' ')?;
    if !is_valid_iso(day) {
        return None;
    }
    let (h, m) = time.split_once(':')?;
    let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    let (y, mo, d) = (day[0..4].parse().ok()?, day[5..7].parse().ok()?, day[8..10].parse().ok()?);
    Some(days_from_civil(y, mo, d) * 1440 + h * 60 + m)
}

/// Minuten seit 1970-01-01 → `JJJJ-MM-TT HH:MM`.
pub fn minutes_to_datetime(minutes: i64) -> String {
    let (y, m, d) = civil_from_days(minutes.div_euclid(1440));
    let tod = minutes.rem_euclid(1440);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", tod / 60, tod % 60)
}

/// `JJJJ-MM-TT HH:MM` → `TT.MM.JJJJ HH:MM`.
pub fn format_datetime_de(s: &str) -> String {
    match s.split_once(' ') {
        Some((day, time)) => format!("{} {time}", format_de(day)),
        None => format_de(s),
    }
}

/// `07:30` → Minuten seit Mitternacht.
pub fn parse_hhmm(s: &str) -> Option<i64> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (i64, i64) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

impl Db {
    /// Aktuelle Zeit (lokal) als `JJJJ-MM-TT HH:MM`.
    pub fn now(&self) -> Result<String> {
        Ok(self.conn.query_row("SELECT strftime('%Y-%m-%d %H:%M', 'now', 'localtime')", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_german_dates() {
        assert_eq!(parse_de("30.09.2026").as_deref(), Some("2026-09-30"));
        assert_eq!(parse_de(" 1.2.26 ").as_deref(), Some("2026-02-01"));
        assert_eq!(parse_de("29.02.2024").as_deref(), Some("2024-02-29"));
        assert_eq!(parse_de("29.02.2025"), None);
        assert_eq!(parse_de("31.04.2026"), None);
        assert_eq!(parse_de("13.13.2026"), None);
        assert_eq!(parse_de("abc"), None);
        assert_eq!(parse_de(""), None);
    }

    #[test]
    fn formats_and_validates() {
        assert_eq!(format_de("2026-09-30"), "30.09.2026");
        assert_eq!(format_de(""), "");
        assert!(is_valid_iso("2026-09-30"));
        assert!(!is_valid_iso("2026-02-30"));
        assert!(!is_valid_iso(""));
        assert_eq!(year_of("2026-09-30"), Some(2026));
    }

    #[test]
    fn datetime_roundtrip_and_arithmetic() {
        let m = datetime_to_minutes("2026-10-25 14:30").unwrap();
        assert_eq!(minutes_to_datetime(m), "2026-10-25 14:30");
        assert_eq!(minutes_to_datetime(m + 10 * 60), "2026-10-26 00:30");
        assert_eq!(minutes_to_datetime(m + 60 * 24 * 7), "2026-11-01 14:30");
        assert_eq!(minutes_to_datetime(datetime_to_minutes("2024-02-28 23:59").unwrap() + 1), "2024-02-29 00:00");
        assert_eq!(datetime_to_minutes("1970-01-01 00:00"), Some(0));
        assert_eq!(datetime_to_minutes("2026-02-30 10:00"), None);
        assert_eq!(datetime_to_minutes("2026-02-10 25:00"), None);
        assert_eq!(datetime_to_minutes("2026-02-10"), None);
        assert_eq!(format_datetime_de("2026-10-25 14:30"), "25.10.2026 14:30");
        assert_eq!(parse_hhmm("07:30"), Some(450));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("x"), None);
    }

    #[test]
    fn sqlite_date_helpers() {
        let db = Db::open_in_memory().unwrap();
        assert!(is_valid_iso(&db.today().unwrap()));
        assert_eq!(db.add_days("2026-09-30", 14).unwrap(), "2026-10-14");
        assert_eq!(db.add_days("2026-01-01", -1).unwrap(), "2025-12-31");
        assert!(db.add_days("kaputt", 1).is_err());
    }
}
