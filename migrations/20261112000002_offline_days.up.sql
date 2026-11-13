-- How many days each product's applications keep working without reaching the server: every
-- token a product's licence gets lasts this long (capped at the licence's expiry). Existing
-- products get 30, the server default until now. It depends on the product's key only (BCNF).
ALTER TABLE products ADD COLUMN offline_days INTEGER NOT NULL DEFAULT 30
    CHECK (offline_days BETWEEN 2 AND 3650);
