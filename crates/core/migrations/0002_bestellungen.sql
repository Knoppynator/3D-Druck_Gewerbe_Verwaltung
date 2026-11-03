-- Datumswerte: ISO-Text 'JJJJ-MM-TT'; leerer Text = nicht gesetzt.
-- Geldbeträge: ganze Cent.

ALTER TABLE purchase_part ADD COLUMN sale_price_cents INTEGER NOT NULL DEFAULT 0 CHECK (sale_price_cents >= 0);

CREATE TABLE sales_order (
    id          INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customer(id),
    received_at TEXT    NOT NULL,
    due_date    TEXT    NOT NULL DEFAULT '',
    channel     TEXT    NOT NULL DEFAULT '',
    note        TEXT    NOT NULL DEFAULT '',
    -- manueller Produktionsstand bis zur ersten Lieferung: 0 Eingegangen, 1 In Produktion, 2 Nachbearbeitet
    stage       INTEGER NOT NULL DEFAULT 0 CHECK (stage BETWEEN 0 AND 2)
);

-- Position: Produkt (+ Material), Zukaufteil oder freie Position
CREATE TABLE order_line (
    id               INTEGER PRIMARY KEY,
    order_id         INTEGER NOT NULL REFERENCES sales_order(id) ON DELETE CASCADE,
    product_id       INTEGER REFERENCES product(id),
    material_id      INTEGER REFERENCES material(id),
    purchase_part_id INTEGER REFERENCES purchase_part(id),
    description      TEXT    NOT NULL,
    unit             TEXT    NOT NULL DEFAULT 'Stück',
    qty              INTEGER NOT NULL CHECK (qty > 0),
    unit_price_cents INTEGER NOT NULL DEFAULT 0,
    CHECK (product_id IS NULL OR purchase_part_id IS NULL)
);

CREATE TABLE shipment (
    id                   INTEGER PRIMARY KEY,
    order_id             INTEGER NOT NULL REFERENCES sales_order(id),
    packed_at            TEXT    NOT NULL,
    shipped_at           TEXT    NOT NULL DEFAULT '',
    shipping_charge_cents INTEGER NOT NULL DEFAULT 0 CHECK (shipping_charge_cents >= 0),
    tracking             TEXT    NOT NULL DEFAULT '',
    note                 TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE shipment_line (
    shipment_id   INTEGER NOT NULL REFERENCES shipment(id) ON DELETE CASCADE,
    order_line_id INTEGER NOT NULL REFERENCES order_line(id),
    qty           INTEGER NOT NULL CHECK (qty > 0),
    PRIMARY KEY (shipment_id, order_line_id)
);

-- Rechnung bzw. Stornorechnung (cancels_invoice_id gesetzt). Inhalt wird beim Erstellen eingefroren.
CREATE TABLE invoice (
    id                 INTEGER PRIMARY KEY,
    number             TEXT    NOT NULL UNIQUE,
    customer_id        INTEGER NOT NULL REFERENCES customer(id),
    shipment_id        INTEGER REFERENCES shipment(id),
    cancels_invoice_id INTEGER REFERENCES invoice(id),
    issued_at          TEXT    NOT NULL,
    due_at             TEXT    NOT NULL,
    recipient          TEXT    NOT NULL,
    salutation         TEXT    NOT NULL,
    footer             TEXT    NOT NULL DEFAULT '',
    total_cents        INTEGER NOT NULL,
    pdf_path           TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE invoice_line (
    invoice_id       INTEGER NOT NULL REFERENCES invoice(id),
    pos              INTEGER NOT NULL,
    description      TEXT    NOT NULL,
    unit             TEXT    NOT NULL,
    qty              INTEGER NOT NULL,
    unit_price_cents INTEGER NOT NULL,
    PRIMARY KEY (invoice_id, pos)
);

CREATE TABLE payment (
    id           INTEGER PRIMARY KEY,
    invoice_id   INTEGER NOT NULL REFERENCES invoice(id),
    paid_at      TEXT    NOT NULL,
    amount_cents INTEGER NOT NULL CHECK (amount_cents <> 0),
    note         TEXT    NOT NULL DEFAULT ''
);

CREATE INDEX idx_order_line_order ON order_line(order_id);
CREATE INDEX idx_shipment_order ON shipment(order_id);
CREATE INDEX idx_invoice_shipment ON invoice(shipment_id);
CREATE INDEX idx_payment_invoice ON payment(invoice_id);
