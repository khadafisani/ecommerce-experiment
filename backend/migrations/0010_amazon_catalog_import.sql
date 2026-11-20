CREATE TABLE amazon_catalog_import (
    id TINYINT PRIMARY KEY CHECK (id = 1),
    fingerprint BINARY(32) NOT NULL,
    shop_id BIGINT NOT NULL REFERENCES shops(id),
    next_byte BIGINT UNSIGNED NOT NULL DEFAULT 0,
    imported BIGINT UNSIGNED NOT NULL DEFAULT 0
) ENGINE=InnoDB;
