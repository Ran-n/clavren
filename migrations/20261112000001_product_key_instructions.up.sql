-- How to get a key for each product, written by the administrator and shown on the product's
-- page at /apply?product=<code>. It depends on the product's key only, so the table stays BCNF.
ALTER TABLE products ADD COLUMN key_instructions TEXT NULL
    CHECK (key_instructions IS NULL OR char_length(key_instructions) <= 2000);
