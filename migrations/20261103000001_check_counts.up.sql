-- How many successful checks applications made, per hour, for the Overview's "checks in the
-- last 24 hours". One row per hour rather than per check: the count is all the Overview needs.
-- The hourly clean-up deletes hours older than 30 days.

CREATE TABLE check_counts (
    hour   TIMESTAMPTZ PRIMARY KEY CHECK (hour = date_trunc('hour', hour)),
    checks BIGINT NOT NULL CHECK (checks > 0)
);
