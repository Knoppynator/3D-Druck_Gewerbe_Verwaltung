-- Lager als Bewegungsjournal: der Bestand ist die Summe der Bewegungen (Zugang +, Abgang −).
-- Posten: Filament (je Material, in Gramm), fertige Druckteile (je Druckteil und Material, Stück),
-- Zukaufteile (Stück).
CREATE TABLE stock_move (
    id               INTEGER PRIMARY KEY,
    kind             TEXT    NOT NULL CHECK (kind IN ('filament', 'print_part', 'purchase_part')),
    material_id      INTEGER REFERENCES material(id),
    print_part_id    INTEGER REFERENCES print_part(id),
    purchase_part_id INTEGER REFERENCES purchase_part(id),
    qty              INTEGER NOT NULL CHECK (qty <> 0),
    moved_at         TEXT    NOT NULL,
    -- receipt (Wareneingang), shipment (Lieferung), inventory (Inventur), scrap (Ausschuss),
    -- production (Druckjob), manual (sonstige Korrektur)
    reason           TEXT    NOT NULL,
    -- Verweis auf den Auslöser, z. B. 'expense' / 'shipment' + Id
    ref_type         TEXT    NOT NULL DEFAULT '',
    ref_id           INTEGER NOT NULL DEFAULT 0,
    note             TEXT    NOT NULL DEFAULT '',
    CHECK (
        (kind = 'filament'      AND material_id IS NOT NULL AND print_part_id IS NULL     AND purchase_part_id IS NULL) OR
        (kind = 'print_part'    AND material_id IS NOT NULL AND print_part_id IS NOT NULL AND purchase_part_id IS NULL) OR
        (kind = 'purchase_part' AND material_id IS NULL     AND print_part_id IS NULL     AND purchase_part_id IS NOT NULL)
    )
);

CREATE INDEX idx_stock_move_item ON stock_move(kind, material_id, print_part_id, purchase_part_id);
CREATE INDEX idx_stock_move_ref ON stock_move(ref_type, ref_id);

-- Anlagenverzeichnis: GWG (sofort abziehbar, bleibt als Ausgabe in der EÜR) oder lineare Abschreibung (AfA).
-- Bei linearer AfA wird die verknüpfte Ausgabe nicht als Betriebsausgabe gezählt, stattdessen die AfA je Jahr.
CREATE TABLE asset (
    id                INTEGER PRIMARY KEY,
    name              TEXT    NOT NULL,
    acquired_at       TEXT    NOT NULL,
    cost_cents        INTEGER NOT NULL CHECK (cost_cents > 0),
    method            TEXT    NOT NULL DEFAULT 'gwg' CHECK (method IN ('gwg', 'linear')),
    useful_life_years INTEGER NOT NULL DEFAULT 0 CHECK (useful_life_years >= 0),
    expense_id        INTEGER REFERENCES expense(id),
    note              TEXT    NOT NULL DEFAULT ''
);
