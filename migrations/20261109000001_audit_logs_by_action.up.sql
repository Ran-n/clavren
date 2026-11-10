-- The audit log's action filter: one action (`action = 'licence.issued'`) or a family
-- (`action LIKE 'licence.%'`). text_pattern_ops serves the prefix too, whatever the collation. The
-- log is kept for ever, so without this a rare action scans all of it.
CREATE INDEX audit_logs_action ON audit_logs (action text_pattern_ops);
