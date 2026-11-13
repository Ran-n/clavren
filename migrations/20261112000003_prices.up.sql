-- Prices: a product's price per key, and what each licence's key was charged. Amounts are whole
-- minor units (cents for EUR) with an ISO 4217 code, never floats. A licence keeps its own price:
-- it is what that key was charged, a fact of the sale, not a copy of the product's price, which
-- may change later. Amount 0 is a key given for free; no price (both NULL) is "not recorded". A
-- test licence is never a sale, so it has no price. Both tables stay BCNF: each price depends on
-- its row's key only.
ALTER TABLE products
    ADD COLUMN price_amount BIGINT NULL,
    ADD COLUMN price_currency TEXT NULL,
    ADD CHECK ((price_amount IS NULL) = (price_currency IS NULL)),
    ADD CHECK (price_amount IS NULL OR price_amount BETWEEN 0 AND 99999999),
    ADD CHECK (price_currency IS NULL OR price_currency ~ '^[A-Z]{3}$');

ALTER TABLE licenses
    ADD COLUMN price_amount BIGINT NULL,
    ADD COLUMN price_currency TEXT NULL,
    ADD CHECK ((price_amount IS NULL) = (price_currency IS NULL)),
    ADD CHECK (price_amount IS NULL OR price_amount BETWEEN 0 AND 99999999),
    ADD CHECK (price_currency IS NULL OR price_currency ~ '^[A-Z]{3}$'),
    ADD CHECK (NOT is_test OR price_amount IS NULL);
