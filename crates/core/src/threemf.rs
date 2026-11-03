//! Liest eine Bambu-Studio-Datei (`.3mf`, auch `.gcode.3mf`).
//!
//! * **Gesliceter Export:** `Metadata/slice_info.config` enthält je Platte Druckzeit, Gewicht, Objekte und Filamente.
//! * **Projektdatei ohne Slicing** (z. B. `15xCover.3mf`): `slice_info.config` ist leer. Dann werden die Objekte
//!   je Platte aus `Metadata/model_settings.config` gelesen und der Filamenttyp aus
//!   `Metadata/project_settings.config`. Druckzeit und Gewicht fehlen (`sliced == false`) und müssen von Hand
//!   eingetragen werden.

use std::{collections::HashMap, io::Read, path::Path};

use crate::{
    catalog::PrintPart,
    db::Db,
    error::{Error, Result},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Filament3mf {
    pub kind: String,
    pub color: String,
    pub used_g: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plate3mf {
    pub index: i64,
    /// Nur bei gesliceten Dateien bekannt (sonst 0)
    pub print_seconds: i64,
    pub weight_g: f64,
    /// Namen der Objekte auf der Platte (ein Eintrag je Exemplar)
    pub objects: Vec<String>,
    pub filaments: Vec<Filament3mf>,
    /// Enthält die Datei Slicing-Ergebnisse (Zeit, Gewicht)?
    pub sliced: bool,
}

/// Ein Objektname mit Anzahl der Exemplare auf der Platte.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectCount {
    pub name: String,
    pub count: i64,
}

impl Plate3mf {
    /// Objekte zusammengefasst nach Namen, in der Reihenfolge des ersten Auftretens.
    pub fn object_counts(&self) -> Vec<ObjectCount> {
        let mut counts: Vec<ObjectCount> = Vec::new();
        for name in &self.objects {
            match counts.iter_mut().find(|c| c.name == *name) {
                Some(c) => c.count += 1,
                None => counts.push(ObjectCount { name: name.clone(), count: 1 }),
            }
        }
        counts
    }
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let found = archive.file_names().filter_map(|n| n.ok()).find(|n| n.eq_ignore_ascii_case(name)).map(|n| n.into_owned())?;
    let mut text = String::new();
    archive.by_name(&found).ok()?.read_to_string(&mut text).ok()?;
    Some(text)
}

/// Liest alle Platten der Datei (mit Slicing-Daten, falls vorhanden).
pub fn read_3mf(path: &Path) -> Result<Vec<Plate3mf>> {
    let bad = |e: &dyn std::fmt::Display| Error::Invalid(format!(".3mf konnte nicht gelesen werden: {e}"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(|e| bad(&e))?;

    if let Some(xml) = read_entry(&mut archive, "Metadata/slice_info.config") {
        let sliced = parse_slice_info(&xml)?;
        if !sliced.is_empty() {
            return Ok(sliced);
        }
    }

    let Some(xml) = read_entry(&mut archive, "Metadata/model_settings.config") else {
        return Err(Error::Invalid(
            "Die Datei enthält weder Slicing-Daten noch Platten – ist es eine Bambu-Studio-Datei (.3mf)?".into(),
        ));
    };
    let filaments = read_entry(&mut archive, "Metadata/project_settings.config")
        .map(|json| project_filaments(&json))
        .unwrap_or_default();
    let mut plates = parse_model_settings(&xml)?;
    for plate in &mut plates {
        plate.filaments = filaments.clone();
    }
    if plates.iter().all(|p| p.objects.is_empty()) {
        return Err(Error::Invalid("Auf den Platten der Datei liegen keine Objekte".into()));
    }
    Ok(plates)
}

/// Filamenttypen und -farben aus `project_settings.config` (JSON).
fn project_filaments(json: &str) -> Vec<Filament3mf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let strings = |key: &str| -> Vec<String> {
        value.get(key).and_then(|v| v.as_array()).map_or(Vec::new(), |a| {
            a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
        })
    };
    let (kinds, colors) = (strings("filament_type"), strings("filament_colour"));
    kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| Filament3mf { kind, color: colors.get(i).cloned().unwrap_or_default(), used_g: 0.0 })
        .collect()
}

fn child_value<'a>(node: roxmltree::Node<'a, 'a>, key: &str) -> Option<&'a str> {
    node.children()
        .filter(|n| n.is_element() && n.tag_name().name() == "metadata")
        .find(|n| n.attribute("key") == Some(key))
        .and_then(|n| n.attribute("value"))
}

/// Platten und Objektnamen aus `model_settings.config` (Projektdatei ohne Slicing).
fn parse_model_settings(xml: &str) -> Result<Vec<Plate3mf>> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| Error::Invalid(format!("model_settings.config ist nicht lesbar: {e}")))?;
    let root = doc.root_element();
    let element = |n: &roxmltree::Node, name: &str| n.is_element() && n.tag_name().name() == name;

    // Objektname je Objekt-Id (nur der Name des Objekts selbst, nicht der Teile darin)
    let names: HashMap<&str, &str> = root
        .children()
        .filter(|n| element(n, "object"))
        .filter_map(|o| Some((o.attribute("id")?, child_value(o, "name")?)))
        .collect();

    let mut plates = Vec::new();
    for plate in root.children().filter(|n| element(n, "plate")) {
        let objects: Vec<String> = plate
            .children()
            .filter(|n| element(n, "model_instance"))
            .filter_map(|i| child_value(i, "object_id"))
            .filter_map(|id| names.get(id).map(|n| n.to_string()))
            .collect();
        plates.push(Plate3mf {
            index: child_value(plate, "plater_id").and_then(|i| i.parse().ok()).unwrap_or(plates.len() as i64 + 1),
            print_seconds: 0,
            weight_g: 0.0,
            objects,
            filaments: Vec::new(),
            sliced: false,
        });
    }
    Ok(plates)
}

/// Platten mit Slicing-Ergebnis aus `slice_info.config` (leer, wenn die Datei nicht gesliced wurde).
fn parse_slice_info(xml: &str) -> Result<Vec<Plate3mf>> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| Error::Invalid(format!("slice_info.config ist nicht lesbar: {e}")))?;
    let mut plates = Vec::new();
    for plate in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "plate") {
        let filaments: Vec<Filament3mf> = plate
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "filament")
            .map(|f| Filament3mf {
                kind: f.attribute("type").unwrap_or_default().to_string(),
                color: f.attribute("color").unwrap_or_default().to_string(),
                used_g: f.attribute("used_g").and_then(|g| g.parse().ok()).unwrap_or(0.0),
            })
            .collect();
        let objects: Vec<String> = plate
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "object")
            .filter(|o| o.attribute("skipped") != Some("true"))
            .filter_map(|o| o.attribute("name").map(str::to_string))
            .collect();
        let weight_g = child_value(plate, "weight")
            .and_then(|w| w.parse().ok())
            .unwrap_or_else(|| filaments.iter().map(|f| f.used_g).sum());
        plates.push(Plate3mf {
            index: child_value(plate, "index").and_then(|i| i.parse().ok()).unwrap_or(plates.len() as i64 + 1),
            print_seconds: child_value(plate, "prediction").and_then(|p| p.parse::<f64>().ok()).map_or(0, |p| p.round() as i64),
            weight_g,
            objects,
            filaments,
            sliced: true,
        });
    }
    Ok(plates)
}

/// Objektname für den Vergleich: klein, ohne Dateiendung, `_`/`-` als Leerzeichen.
fn normalize(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let stem = ["stl", "step", "stp", "obj", "3mf"]
        .iter()
        .find_map(|ext| lower.strip_suffix(&format!(".{ext}")))
        .unwrap_or(&lower);
    stem.replace(['_', '-'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Schlägt ein Druckteil für einen Objektnamen vor: gleicher Name, sonst Teilname im Objektnamen (längster
/// gewinnt), sonst Objektname im Teilnamen (kürzester gewinnt: „Main Body“ → „Housing Main Body“).
pub fn guess_part(object: &str, parts: &[PrintPart]) -> Option<i64> {
    let object = normalize(object);
    if object.len() < 2 {
        return None;
    }
    let named: Vec<(i64, String)> = parts.iter().map(|p| (p.id, normalize(&p.name))).filter(|(_, n)| !n.is_empty()).collect();
    if let Some((id, _)) = named.iter().find(|(_, n)| *n == object) {
        return Some(*id);
    }
    if let Some((id, _)) = named.iter().filter(|(_, n)| object.contains(n.as_str())).max_by_key(|(_, n)| n.len()) {
        return Some(*id);
    }
    named.iter().filter(|(_, n)| n.contains(object.as_str())).min_by_key(|(_, n)| n.len()).map(|(id, _)| *id)
}

/// Fasst die Objekte einer Platte zu Druckteilen zusammen: `(Teil-Id, Anzahl)`. Objekte ohne Zuordnung
/// (Hilfskörper wie Verbinder-Würfel) bleiben außen vor.
pub fn parts_from_objects(counts: &[ObjectCount], mapping: &HashMap<String, i64>) -> Vec<(i64, i64)> {
    let mut parts: Vec<(i64, i64)> = Vec::new();
    for c in counts {
        let Some(&part_id) = mapping.get(&c.name) else { continue };
        if part_id == 0 {
            continue;
        }
        match parts.iter_mut().find(|(id, _)| *id == part_id) {
            Some((_, n)) => *n += c.count,
            None => parts.push((part_id, c.count)),
        }
    }
    parts
}

impl Db {
    /// Bisher gewählte Zuordnung eines Objektnamens: `Some(0)` = ausdrücklich ignorieren.
    pub fn remembered_part_for_object(&self, object: &str) -> Result<Option<i64>> {
        Ok(self.setting(&format!("map3mf:{}", normalize(object)))?.and_then(|v| v.parse().ok()))
    }

    /// Merkt die Zuordnung eines Objektnamens (`0` = ignorieren) für künftige Importe.
    pub fn remember_part_for_object(&self, object: &str, part_id: i64) -> Result<()> {
        self.set_setting(&format!("map3mf:{}", normalize(object)), &part_id.to_string())
    }

    /// Vorbelegung für alle Objektnamen: gemerkte Zuordnung, sonst Namensvergleich, sonst ignorieren (0).
    pub fn object_mapping(&self, objects: &[ObjectCount], parts: &[PrintPart]) -> Result<HashMap<String, i64>> {
        let mut mapping = HashMap::new();
        for o in objects {
            let remembered = self.remembered_part_for_object(&o.name)?.filter(|id| *id == 0 || parts.iter().any(|p| p.id == *id));
            mapping.insert(o.name.clone(), remembered.or_else(|| guess_part(&o.name, parts)).unwrap_or(0));
        }
        Ok(mapping)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const SLICED: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <header><header_item key="X-BBL-Client-Type" value="slicer"/></header>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="prediction" value="5412"/>
    <metadata key="weight" value="48.75"/>
    <object identify_id="11" name="Housing_v3.stl" skipped="false"/>
    <object identify_id="12" name="Housing_v3.stl" skipped="false"/>
    <object identify_id="13" name="Cap.stl" skipped="false"/>
    <object identify_id="14" name="Sled.stl" skipped="true"/>
    <filament id="1" type="PETG" color="#000000" used_m="16.3" used_g="48.75"/>
  </plate>
  <plate>
    <metadata key="index" value="2"/>
    <metadata key="prediction" value="600.4"/>
    <filament id="1" type="PETG-CF" color="#222222" used_g="10.5"/>
    <filament id="2" type="PETG" color="#ffffff" used_g="1.5"/>
  </plate>
</config>"##;

    const UNSLICED: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <header><header_item key="X-BBL-Client-Type" value="slicer"/></header>
</config>"##;

    const MODEL_SETTINGS: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <object id="2"><metadata key="name" value="Sled.stl"/>
    <part id="1"><metadata key="name" value="Teil-Name-der-nicht-zaehlt"/></part></object>
  <object id="3"><metadata key="name" value="Sled.stl"/></object>
  <object id="4"><metadata key="name" value="Würfel"/></object>
  <plate>
    <metadata key="plater_id" value="1"/>
    <model_instance><metadata key="object_id" value="2"/><metadata key="instance_id" value="0"/></model_instance>
    <model_instance><metadata key="object_id" value="3"/><metadata key="instance_id" value="0"/></model_instance>
    <model_instance><metadata key="object_id" value="4"/><metadata key="instance_id" value="0"/></model_instance>
  </plate>
</config>"##;

    fn part(id: i64, name: &str) -> PrintPart {
        PrintPart { id, name: name.into(), ..Default::default() }
    }

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, content) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("druckplaner_3mf_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_plates_from_slice_info() {
        let plates = parse_slice_info(SLICED).unwrap();
        assert_eq!(plates.len(), 2);
        let p1 = &plates[0];
        assert!(p1.sliced);
        assert_eq!((p1.index, p1.print_seconds), (1, 5412));
        assert!((p1.weight_g - 48.75).abs() < 1e-9);
        assert_eq!(p1.objects, vec!["Housing_v3.stl", "Housing_v3.stl", "Cap.stl"], "übersprungene Objekte entfallen");
        assert_eq!(
            p1.object_counts(),
            vec![
                ObjectCount { name: "Housing_v3.stl".into(), count: 2 },
                ObjectCount { name: "Cap.stl".into(), count: 1 }
            ]
        );
        assert_eq!(p1.filaments, vec![Filament3mf { kind: "PETG".into(), color: "#000000".into(), used_g: 48.75 }]);
        let p2 = &plates[1];
        assert_eq!(p2.print_seconds, 600);
        assert!((p2.weight_g - 12.0).abs() < 1e-9, "Gewicht aus der Summe der Filamente");
        assert!(parse_slice_info(UNSLICED).unwrap().is_empty(), "ohne Slicing keine Platten");
        assert!(parse_slice_info("kein xml").is_err());
    }

    #[test]
    fn model_settings_give_objects_per_plate_without_slicing() {
        let plates = parse_model_settings(MODEL_SETTINGS).unwrap();
        assert_eq!(plates.len(), 1);
        assert!(!plates[0].sliced);
        assert_eq!(plates[0].objects, vec!["Sled.stl", "Sled.stl", "Würfel"], "Objektnamen, nicht Teilnamen");
        assert_eq!(plates[0].print_seconds, 0);
    }

    #[test]
    fn reads_sliced_and_unsliced_files_from_zip() {
        let dir = temp_dir("zip");
        let sliced = dir.join("test.gcode.3mf");
        write_zip(&sliced, &[("[Content_Types].xml", "<Types/>"), ("Metadata/slice_info.config", SLICED)]);
        let plates = read_3mf(&sliced).unwrap();
        assert_eq!((plates.len(), plates[0].sliced), (2, true));

        let project = dir.join("projekt.3mf");
        write_zip(
            &project,
            &[
                ("Metadata/slice_info.config", UNSLICED),
                ("Metadata/model_settings.config", MODEL_SETTINGS),
                ("Metadata/project_settings.config", r##"{"filament_type": ["PETG"], "filament_colour": ["#161616"]}"##),
            ],
        );
        let plates = read_3mf(&project).unwrap();
        assert_eq!(plates.len(), 1);
        assert!(!plates[0].sliced);
        assert_eq!(plates[0].filaments, vec![Filament3mf { kind: "PETG".into(), color: "#161616".into(), used_g: 0.0 }]);

        let foreign = dir.join("fremd.3mf");
        write_zip(&foreign, &[("3D/3dmodel.model", "<model/>")]);
        let err = read_3mf(&foreign).unwrap_err().to_string();
        assert!(err.contains("weder Slicing-Daten noch Platten"), "{err}");
        assert!(read_3mf(&dir.join("gibt-es-nicht.3mf")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn objects_are_matched_to_print_parts() {
        let parts = [part(1, "Housing"), part(2, "Cap"), part(3, "Housing Cap"), part(4, "Housing Main Body")];
        assert_eq!(guess_part("Housing_v3.stl", &parts), Some(1));
        assert_eq!(guess_part("housing_v3.stl", &parts), Some(1));
        assert_eq!(guess_part("Cap.stl", &parts), Some(2));
        assert_eq!(guess_part("Housing Cap.stl", &parts), Some(3), "längster Teilname gewinnt");
        assert_eq!(guess_part("Main_Body.stl", &parts), Some(4), "Objektname steckt im Teilnamen");
        assert_eq!(guess_part("Würfel", &parts), None);
        assert_eq!(guess_part("x", &parts), None);
        assert_eq!(guess_part("Cap.stl", &[]), None);

        let counts = vec![
            ObjectCount { name: "Housing_v3.stl".into(), count: 2 },
            ObjectCount { name: "Main_Body.stl".into(), count: 3 },
            ObjectCount { name: "Housing_v4.stl".into(), count: 1 },
            ObjectCount { name: "Würfel".into(), count: 2 },
        ];
        let mapping: HashMap<String, i64> = [
            ("Housing_v3.stl", 1),
            ("Main_Body.stl", 4),
            ("Housing_v4.stl", 1),
            ("Würfel", 0),
        ]
        .into_iter()
        .map(|(n, id)| (n.to_string(), id))
        .collect();
        assert_eq!(parts_from_objects(&counts, &mapping), vec![(1, 3), (4, 3)], "gleiches Teil wird summiert, 0 ignoriert");
    }

    #[test]
    fn mapping_is_remembered_between_imports() {
        let db = Db::open_in_memory().unwrap();
        let mut cap = PrintPart { name: "Cap".into(), ..Default::default() };
        db.save_print_part(&mut cap).unwrap();
        let parts = db.list_print_parts().unwrap();
        let counts = vec![
            ObjectCount { name: "Lid.stl".into(), count: 25 },
            ObjectCount { name: "Würfel".into(), count: 2 },
            ObjectCount { name: "Cap.stl".into(), count: 1 },
        ];
        let first = db.object_mapping(&counts, &parts).unwrap();
        assert_eq!((first["Lid.stl"], first["Würfel"], first["Cap.stl"]), (0, 0, cap.id));

        db.remember_part_for_object("Lid.stl", cap.id).unwrap();
        db.remember_part_for_object("Würfel", 0).unwrap();
        let second = db.object_mapping(&counts, &parts).unwrap();
        assert_eq!((second["Lid.stl"], second["Würfel"]), (cap.id, 0));

        // gemerktes, inzwischen gelöschtes Teil wird nicht mehr vorgeschlagen
        db.remember_part_for_object("Lid.stl", 9999).unwrap();
        assert_eq!(db.object_mapping(&counts, &parts).unwrap()["Lid.stl"], 0);
    }

    /// Prüft gegen die echten Beispieldateien aus `ressourcen/Printer_project_files`, sofern vorhanden.
    #[test]
    fn real_bambu_project_files_are_readable() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ressourcen/Printer_project_files");
        if !dir.is_dir() {
            return;
        }
        let expected = [
            ("15xCover.3mf", "Cover.stl", 15),
            ("15xMain_Body.3mf", "Main_Body.stl", 15),
            ("16xSled.3mf", "Sled.stl", 16),
            ("25xLid.3mf", "Lid.stl", 26),
            ("8xMain_Body.3mf", "Main_Body.stl", 8),
            ("8xSled.3mf", "Sled.stl", 8),
        ];
        for (file, object, count) in expected {
            let plates = read_3mf(&dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(plates.len(), 1, "{file}");
            assert!(!plates[0].sliced, "{file} enthält keine Slicing-Daten");
            let found = plates[0].object_counts().into_iter().find(|c| c.name == object).unwrap_or_else(|| panic!("{file}: {object}"));
            assert_eq!(found.count, count, "{file}");
            assert_eq!(plates[0].filaments.first().map(|f| f.kind.as_str()), Some("PETG"), "{file}");
        }
    }
}
