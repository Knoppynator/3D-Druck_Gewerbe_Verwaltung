-- Anzeigefarbe des Filaments (#RRGGBB, leer = aus dem Farbnamen raten).
ALTER TABLE material ADD COLUMN color_hex TEXT NOT NULL DEFAULT '';

-- Druckstart-Zeiten ersetzen das eine Plattenwechsel-Fenster: das bisherige Fenster wird übernommen.
INSERT OR IGNORE INTO setting (key, value)
SELECT 'start_times',
       COALESCE((SELECT value FROM setting WHERE key = 'plate_window_from'), '07:00') || '-' ||
       COALESCE((SELECT value FROM setting WHERE key = 'plate_window_to'), '23:00')
WHERE EXISTS (SELECT 1 FROM setting WHERE key IN ('plate_window_from', 'plate_window_to'));
DELETE FROM setting WHERE key IN ('plate_window_from', 'plate_window_to');
