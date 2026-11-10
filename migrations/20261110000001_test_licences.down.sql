-- Test licences become ordinary ones. The one-active-computer index can't hold while one is
-- active on several, so every computer of a licence but its latest is reset first.
UPDATE activations a SET revoked_at = now(), revocation_reason = 'reset'
WHERE a.revoked_at IS NULL
  AND EXISTS (
      SELECT 1 FROM activations later
      WHERE later.license_id = a.license_id AND later.revoked_at IS NULL
        AND (later.activated_at, later.id) > (a.activated_at, a.id)
  );

DROP VIEW license_states;
CREATE VIEW license_states AS
SELECT l.id AS license_id,
       CASE
           WHEN l.revoked_at IS NOT NULL THEN 'revoked'
           WHEN a.id IS NOT NULL THEN 'redeemed'
           ELSE 'unused'
       END AS state,
       a.id AS active_activation_id,
       a.activated_at AS redeemed_at
FROM licenses l
LEFT JOIN activations a ON a.license_id = l.id AND a.revoked_at IS NULL;

DROP TRIGGER activations_one_active ON activations;
DROP FUNCTION activations_one_active_unless_test();
DROP INDEX activations_active_by_license;
CREATE UNIQUE INDEX activations_one_active_per_license ON activations (license_id) WHERE revoked_at IS NULL;

DROP TRIGGER licenses_is_test_immutable ON licenses;
DROP FUNCTION licenses_is_test_is_immutable();
ALTER TABLE licenses DROP COLUMN is_test;
