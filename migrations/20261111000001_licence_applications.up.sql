-- Licence applications: someone asks for a licence, from an application or the web form, and the
-- administrator approves (which issues the licence) or rejects it. The applicant holds a random
-- secret and asks with it how the application went; only its BLAKE3 hash is stored.
ALTER TABLE products ADD COLUMN accepts_applications BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE license_applications (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- deleting a product deletes its applications
    product_id       UUID NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    name             TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    email            TEXT NOT NULL CHECK (length(email) BETWEEN 3 AND 320),
    message          TEXT CHECK (length(message) <= 2000),
    source           TEXT NOT NULL CHECK (source IN ('app', 'web')),
    secret_hash      BYTEA NOT NULL UNIQUE CHECK (length(secret_hash) = 32),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at       TIMESTAMPTZ,
    decision         TEXT CHECK (decision IN ('approved', 'rejected')),
    -- the licence an approval issued; a licence deleted since leaves it empty
    license_id       UUID UNIQUE REFERENCES licenses (id) ON DELETE SET NULL,
    rejection_reason TEXT CHECK (length(rejection_reason) <= 500),
    -- when the applicant first collected the key
    key_collected_at TIMESTAMPTZ,
    CHECK ((decided_at IS NULL) = (decision IS NULL)),
    CHECK (license_id IS NULL OR decision IS NOT DISTINCT FROM 'approved'),
    CHECK (rejection_reason IS NULL OR decision IS NOT DISTINCT FROM 'rejected'),
    CHECK (key_collected_at IS NULL OR decision IS NOT DISTINCT FROM 'approved')
);
-- one waiting application per product and address: a second one is refused until it is decided
CREATE UNIQUE INDEX license_applications_one_pending
    ON license_applications (product_id, lower(email)) WHERE decided_at IS NULL;
CREATE INDEX license_applications_product ON license_applications (product_id);
