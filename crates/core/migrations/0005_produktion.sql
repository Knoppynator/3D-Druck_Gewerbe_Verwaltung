-- Plattenvorlage: eine Druckplatte (aus dem Slicer) mit Druckzeit, Filamentverbrauch und den Teilen darauf.
-- Eine Platte ist immer einfarbig; das Material wird erst beim Druckjob gewählt.
CREATE TABLE plate_template (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL UNIQUE,
    -- Werkstoff, für den die Platte geslict wurde (z. B. PETG); leer = beliebig
    material_kind TEXT    NOT NULL DEFAULT '',
    print_seconds INTEGER NOT NULL CHECK (print_seconds > 0),
    filament_g    REAL    NOT NULL DEFAULT 0 CHECK (filament_g >= 0),
    -- Kopie der .3mf-Datei, relativ zum Datenordner
    file_3mf      TEXT    NOT NULL DEFAULT '',
    note          TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE plate_template_part (
    plate_template_id INTEGER NOT NULL REFERENCES plate_template(id) ON DELETE CASCADE,
    print_part_id     INTEGER NOT NULL REFERENCES print_part(id),
    qty               INTEGER NOT NULL CHECK (qty > 0),
    PRIMARY KEY (plate_template_id, print_part_id)
);

-- Druckjob = ein Druck einer Platte in einer Farbe. Warteschlange nach `position`.
-- Zeitstempel als 'JJJJ-MM-TT HH:MM'.
CREATE TABLE print_job (
    id                INTEGER PRIMARY KEY,
    plate_template_id INTEGER NOT NULL REFERENCES plate_template(id),
    material_id       INTEGER NOT NULL REFERENCES material(id),
    status            TEXT    NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'printing', 'done', 'failed')),
    position          INTEGER NOT NULL,
    started_at        TEXT    NOT NULL DEFAULT '',
    finished_at       TEXT    NOT NULL DEFAULT '',
    -- tatsächlich verbrauchtes Filament in Gramm (bei Abschluss)
    used_g            INTEGER NOT NULL DEFAULT 0,
    note              TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_print_job_status ON print_job(status, position);

-- Gewünschter Mindestbestand fertiger Druckteile je Farbe (füllt freie Druckerzeit auf)
CREATE TABLE finished_min (
    print_part_id INTEGER NOT NULL REFERENCES print_part(id),
    material_id   INTEGER NOT NULL REFERENCES material(id),
    min_qty       INTEGER NOT NULL CHECK (min_qty > 0),
    PRIMARY KEY (print_part_id, material_id)
);
