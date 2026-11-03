-- Positionsart einer Rechnungszeile: 'product' (Produkt/Set), 'part' (Zukaufteil), 'shipping' (Lieferkosten),
-- 'legacy' (Sammelposition einer importierten Altrechnung), 'other'.
ALTER TABLE invoice_line ADD COLUMN kind TEXT NOT NULL DEFAULT 'other';

-- Bereits vorhandene Zeilen einordnen (Produktzeilen tragen Produkt und Material, getrennt durch einen Zeilenumbruch)
UPDATE invoice_line SET kind = 'shipping' WHERE description = 'Lieferkosten';
UPDATE invoice_line SET kind = 'legacy' WHERE description LIKE 'Altrechnung%';
UPDATE invoice_line SET kind = 'product' WHERE kind = 'other' AND instr(description, char(10)) > 0;

-- Stückzahl einer Rechnung zum Nachtragen, wo sie sich nicht aus den Positionen ergibt (Altrechnungen);
-- NULL = Summe der Produktzeilen
ALTER TABLE invoice ADD COLUMN units INTEGER;
