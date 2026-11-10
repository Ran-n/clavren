-- Test licences: for trying an application against the real server. They can be active on
-- several computers at once, are left out of the Overview's numbers, and their tokens say so.
-- A licence is a test licence from issuing to deletion (the trigger below keeps it that way).
ALTER TABLE licenses ADD COLUMN is_test BOOLEAN NOT NULL DEFAULT FALSE;

CREATE FUNCTION licenses_is_test_is_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.is_test <> OLD.is_test THEN
        RAISE EXCEPTION 'a licence can''t become or stop being a test licence'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER licenses_is_test_immutable
BEFORE UPDATE OF is_test ON licenses
FOR EACH ROW EXECUTE FUNCTION licenses_is_test_is_immutable();

-- One active computer per licence, except test licences. A unique index can't look at the licence,
-- so a trigger keeps the rule: it locks the licence row first (as activating does), so two
-- activations of one key are still decided one after the other, and the second one sees the first.
DROP INDEX activations_one_active_per_license;
CREATE INDEX activations_active_by_license ON activations (license_id) WHERE revoked_at IS NULL;

CREATE FUNCTION activations_one_active_unless_test() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    test BOOLEAN;
BEGIN
    IF NEW.revoked_at IS NOT NULL THEN
        RETURN NEW;
    END IF;
    SELECT is_test INTO test FROM licenses WHERE id = NEW.license_id FOR UPDATE;
    IF NOT test AND EXISTS (
        SELECT 1 FROM activations WHERE license_id = NEW.license_id AND revoked_at IS NULL AND id <> NEW.id
    ) THEN
        RAISE EXCEPTION 'the licence is already active on another computer'
            USING ERRCODE = 'unique_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER activations_one_active
BEFORE INSERT OR UPDATE OF license_id, revoked_at ON activations
FOR EACH ROW EXECUTE FUNCTION activations_one_active_unless_test();

-- With several active computers, the view names the latest one and counts them all.
DROP VIEW license_states;
CREATE VIEW license_states AS
SELECT l.id AS license_id,
       CASE
           WHEN l.revoked_at IS NOT NULL THEN 'revoked'
           WHEN a.id IS NOT NULL THEN 'redeemed'
           ELSE 'unused'
       END AS state,
       a.id AS active_activation_id,
       a.activated_at AS redeemed_at,
       COALESCE(a.computers, 0) AS active_computers
FROM licenses l
LEFT JOIN LATERAL (
    SELECT id, activated_at, count(*) OVER () AS computers
    FROM activations
    WHERE license_id = l.id AND revoked_at IS NULL
    ORDER BY activated_at DESC, id DESC
    LIMIT 1
) a ON TRUE;
