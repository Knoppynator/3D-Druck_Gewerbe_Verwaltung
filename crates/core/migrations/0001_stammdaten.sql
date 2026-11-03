-- Geldbeträge der Buchhaltung: ganze Cent (INTEGER).
-- Einkaufspreise einzelner Zukaufteile (z. B. 0,053 €/Schraube) sind reine Kalkulationswerte
-- und daher als REAL in Euro abgelegt.

CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE customer (
    id         INTEGER PRIMARY KEY,
    company    TEXT NOT NULL,
    contact    TEXT NOT NULL DEFAULT '',
    salutation TEXT NOT NULL DEFAULT '',
    street     TEXT NOT NULL DEFAULT '',
    zip        TEXT NOT NULL DEFAULT '',
    city       TEXT NOT NULL DEFAULT '',
    note       TEXT NOT NULL DEFAULT ''
);

-- Filament-Sorte: Material (PETG, PETG-CF ...) + Farbe
CREATE TABLE material (
    id                 INTEGER PRIMARY KEY,
    kind               TEXT    NOT NULL,
    color              TEXT    NOT NULL,
    vendor             TEXT    NOT NULL DEFAULT '',
    price_per_kg_cents INTEGER NOT NULL DEFAULT 0 CHECK (price_per_kg_cents >= 0),
    min_stock_g        INTEGER NOT NULL DEFAULT 0 CHECK (min_stock_g >= 0),
    lead_time_days     INTEGER NOT NULL DEFAULT 0 CHECK (lead_time_days >= 0),
    UNIQUE (kind, color)
);

CREATE TABLE print_part (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    notes TEXT NOT NULL DEFAULT ''
);

CREATE TABLE purchase_part (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL UNIQUE,
    unit         TEXT    NOT NULL DEFAULT 'Stück',
    unit_cost_eur REAL   NOT NULL DEFAULT 0 CHECK (unit_cost_eur >= 0),
    min_stock    INTEGER NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
    sellable     INTEGER NOT NULL DEFAULT 0 CHECK (sellable IN (0, 1))
);

CREATE TABLE product (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL UNIQUE,
    labor_minutes INTEGER NOT NULL DEFAULT 0 CHECK (labor_minutes >= 0)
);

-- Stückliste: je Zeile entweder ein Druckteil oder ein Zukaufteil
CREATE TABLE product_bom (
    product_id       INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    print_part_id    INTEGER REFERENCES print_part(id),
    purchase_part_id INTEGER REFERENCES purchase_part(id),
    qty              INTEGER NOT NULL CHECK (qty > 0),
    CHECK ((print_part_id IS NULL) <> (purchase_part_id IS NULL))
);

-- Staffelpreis: gilt ab min_qty Stück (Gesamtmenge des Produkts in einer Bestellung)
CREATE TABLE price_tier (
    product_id       INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    min_qty          INTEGER NOT NULL CHECK (min_qty > 0),
    unit_price_cents INTEGER NOT NULL CHECK (unit_price_cents >= 0),
    PRIMARY KEY (product_id, min_qty)
);

-- Aufpreis je Stück für ein Material (z. B. PETG-CF)
CREATE TABLE material_surcharge (
    product_id    INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    material_kind TEXT    NOT NULL,
    cents         INTEGER NOT NULL,
    PRIMARY KEY (product_id, material_kind)
);
