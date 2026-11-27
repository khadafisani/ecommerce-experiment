CREATE TABLE search_keyword_terms (
    id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin PRIMARY KEY,
    phrase VARCHAR(180) NOT NULL,
    demand BIGINT UNSIGNED NOT NULL DEFAULT 0,
    aliases JSON NOT NULL DEFAULT '[]',
    dirty BOOLEAN NOT NULL DEFAULT TRUE,
    INDEX keyword_publish (dirty,id)
) ENGINE=InnoDB;

CREATE TABLE search_canonical_products (
    id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin PRIMARY KEY,
    keyword_id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    brand VARCHAR(120) NOT NULL,
    category VARCHAR(120) NOT NULL,
    embedding JSON,
    embedded BOOLEAN NOT NULL DEFAULT FALSE,
    dirty BOOLEAN NOT NULL DEFAULT TRUE,
    INDEX canonical_publish (dirty,id),
    INDEX canonical_embedding (embedded,id),
    INDEX canonical_keyword (keyword_id)
) ENGINE=InnoDB;

CREATE TABLE search_catalog_sources (
    product_id BIGINT NOT NULL,
    variant_id BIGINT NOT NULL,
    canonical_id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    keyword_id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    catalog_fingerprint CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL DEFAULT '',
    PRIMARY KEY (product_id,variant_id),
    INDEX source_group (canonical_id,variant_id,keyword_id),
    INDEX source_keyword (keyword_id)
) ENGINE=InnoDB;

CREATE TABLE search_catalog_jobs (
    product_id BIGINT PRIMARY KEY,
    revision BIGINT UNSIGNED NOT NULL DEFAULT 1,
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    INDEX catalog_job_order (updated_at,product_id)
) ENGINE=InnoDB;

CREATE TABLE search_query_cleanup (
    id CHAR(64) CHARACTER SET ascii COLLATE ascii_bin PRIMARY KEY,
    phrase VARCHAR(150) NOT NULL,
    submissions BIGINT UNSIGNED NOT NULL DEFAULT 1,
    counted BIGINT UNSIGNED NOT NULL DEFAULT 0,
    ready BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    INDEX query_cleanup_age (created_at,id),
    INDEX query_cleanup_due (ready,created_at,id)
) ENGINE=InnoDB;

CREATE TABLE search_catalog_cursor (
    id TINYINT PRIMARY KEY,
    last_product_id BIGINT NOT NULL,
    backfill_complete BOOLEAN NOT NULL DEFAULT FALSE
) ENGINE=InnoDB;
INSERT INTO search_catalog_cursor (id,last_product_id) VALUES (1,0);

-- Extend the existing durable search sync to include changes to searchable option text.
CREATE OR REPLACE TRIGGER search_variant_update
AFTER UPDATE ON variants FOR EACH ROW
BEGIN
    IF OLD.enabled AND (NEW.product_id <> OLD.product_id OR NOT NEW.enabled) THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',OLD.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6);
    END IF;
    IF NEW.enabled AND (NEW.product_id <> OLD.product_id OR NOT OLD.enabled
       OR NEW.price <> OLD.price OR NOT (BINARY NEW.name <=> BINARY OLD.name)
       OR NOT (BINARY NEW.subvariant <=> BINARY OLD.subvariant)) THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',NEW.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;
