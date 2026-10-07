CREATE TABLE lot (
    id     INTEGER PRIMARY KEY,
    ticker TEXT    NOT NULL CHECK (ticker <> '' AND ticker = upper(ticker)),
    bought TEXT    NOT NULL,                     -- ISO YYYY-MM-DD
    shares INTEGER NOT NULL CHECK (shares > 0),  -- thousandths of a share
    price  INTEGER NOT NULL CHECK (price > 0)    -- cents per share
);

-- `value IS NULL` is a plan: shares reserved for a donation not yet made.
CREATE TABLE donation (
    id      INTEGER PRIMARY KEY,
    ticker  TEXT    NOT NULL CHECK (ticker <> '' AND ticker = upper(ticker)),
    date    TEXT    NOT NULL,
    shares  INTEGER NOT NULL CHECK (shares > 0),
    value   INTEGER CHECK (value IS NULL OR value > 0),
    claimed INTEGER NOT NULL DEFAULT 0 CHECK (claimed IN (0, 1)),
    CHECK (value IS NOT NULL OR claimed = 0)
);

-- No cascade from `lot`: a lot a donation draws on cannot be deleted.
CREATE TABLE allocation (
    lot_id      INTEGER NOT NULL REFERENCES lot(id),
    donation_id INTEGER NOT NULL REFERENCES donation(id) ON DELETE CASCADE,
    shares      INTEGER NOT NULL CHECK (shares > 0),
    manual      INTEGER NOT NULL DEFAULT 0 CHECK (manual IN (0, 1)),
    PRIMARY KEY (lot_id, donation_id)
);

CREATE TABLE price (
    ticker TEXT    NOT NULL,
    date   TEXT    NOT NULL,
    price  INTEGER NOT NULL CHECK (price > 0),
    PRIMARY KEY (ticker, date)
);
