-- Ausgaben mit Kategorien (für die EÜR) und Belegen. Einnahmen ergeben sich aus den Zahlungen (payment).

-- Ausgaben-Kategorie; euer_line/euer_label ordnen sie einer Zeile der Anlage EÜR zu (vom Benutzer zu pflegen,
-- da sich die Zeilennummern je Steuerjahr ändern können).
CREATE TABLE category (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    euer_line  TEXT NOT NULL DEFAULT '',
    euer_label TEXT NOT NULL DEFAULT ''
);

INSERT INTO category (name, euer_label) VALUES
    ('Material & Zukaufteile', 'Waren, Rohstoffe und Hilfsstoffe einschl. der Nebenkosten'),
    ('Versand & Porto',        'Übrige Betriebsausgaben (Porto, Versandmaterial)'),
    ('Werkzeug & Geräte',      'Geringwertige Wirtschaftsgüter (GWG, bis 800 € netto)'),
    ('Sonstiges',              'Übrige Betriebsausgaben');

CREATE TABLE expense (
    id              INTEGER PRIMARY KEY,
    spent_at        TEXT    NOT NULL,
    vendor          TEXT    NOT NULL DEFAULT '',
    description     TEXT    NOT NULL,
    qty             INTEGER NOT NULL DEFAULT 1 CHECK (qty >= 0),
    category_id     INTEGER NOT NULL REFERENCES category(id),
    amount_cents    INTEGER NOT NULL CHECK (amount_cents <> 0),
    -- relativ zum Datenordner (belege/JJJJ/...); leer = keine Datei verknüpft
    receipt_path    TEXT    NOT NULL DEFAULT '',
    -- Beleg liegt vor (auch ohne verknüpfte Datei, z. B. auf Papier)
    receipt_present INTEGER NOT NULL DEFAULT 0 CHECK (receipt_present IN (0, 1)),
    shipment_id     INTEGER REFERENCES shipment(id),
    note            TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_expense_date ON expense(spent_at);
