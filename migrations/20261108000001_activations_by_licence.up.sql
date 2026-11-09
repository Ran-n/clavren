-- Every activation of a licence, history included: deleting licences (and their product) removes
-- their activations, and the foreign key from activations checks the same. The partial unique
-- index on active activations can't serve that, so without this both scan the table.
CREATE INDEX activations_by_license ON activations (license_id);
