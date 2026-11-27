-- Revisit previously extracted titles using the new catalog extraction fingerprint.
UPDATE search_catalog_cursor SET last_product_id=0,backfill_complete=FALSE WHERE id=1;
