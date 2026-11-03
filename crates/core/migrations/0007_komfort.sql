-- Staffelpreis und Lieferzeit-Staffel: Menge je Produkt UND Farbe zählen (statt Gesamtmenge über alle Farben).
ALTER TABLE product ADD COLUMN tier_per_color INTEGER NOT NULL DEFAULT 0 CHECK (tier_per_color IN (0, 1));

-- Standard-Lieferzeit je Produkt, gestaffelt nach Stückzahl (gilt ab min_qty Stück) – Vorschlag für den Wunschtermin.
CREATE TABLE lead_tier (
    product_id INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    min_qty    INTEGER NOT NULL CHECK (min_qty > 0),
    days       INTEGER NOT NULL CHECK (days >= 0),
    PRIMARY KEY (product_id, min_qty)
);
