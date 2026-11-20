-- Coalesce catalog changes in the same InnoDB transactions that make them visible.
CREATE TABLE IF NOT EXISTS search_sync_jobs (
    entity_type ENUM('product','shop') NOT NULL,
    entity_id BIGINT NOT NULL,
    revision BIGINT UNSIGNED NOT NULL DEFAULT 1,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (entity_type,entity_id),
    INDEX search_sync_due (entity_type,updated_at,entity_id)
) ENGINE=InnoDB;

CREATE OR REPLACE TRIGGER search_product_insert
AFTER INSERT ON products FOR EACH ROW
BEGIN
    INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
END;

CREATE OR REPLACE TRIGGER search_product_update
AFTER UPDATE ON products FOR EACH ROW
BEGIN
    IF NEW.id <> OLD.id THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',OLD.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
    IF NEW.id <> OLD.id OR NOT (NEW.shop_id <=> OLD.shop_id)
       OR NOT (NEW.brand_id <=> OLD.brand_id)
       OR NOT (NEW.category_id <=> OLD.category_id)
       OR NOT (BINARY NEW.name <=> BINARY OLD.name)
       OR NOT (BINARY NEW.description <=> BINARY OLD.description)
       OR NOT (BINARY NEW.image <=> BINARY OLD.image)
       OR NEW.status <> OLD.status THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

CREATE OR REPLACE TRIGGER search_product_delete
AFTER DELETE ON products FOR EACH ROW
BEGIN
    INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',OLD.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
END;

CREATE OR REPLACE TRIGGER search_variant_insert
AFTER INSERT ON variants FOR EACH ROW
BEGIN
    IF NEW.enabled THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',NEW.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

CREATE OR REPLACE TRIGGER search_variant_update
AFTER UPDATE ON variants FOR EACH ROW
BEGIN
    IF OLD.enabled AND (NEW.product_id <> OLD.product_id OR NOT NEW.enabled) THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',OLD.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
    IF NEW.enabled AND (NEW.product_id <> OLD.product_id OR NOT OLD.enabled OR NEW.price <> OLD.price) THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',NEW.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

CREATE OR REPLACE TRIGGER search_variant_delete
AFTER DELETE ON variants FOR EACH ROW
BEGIN
    IF OLD.enabled THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',OLD.product_id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

CREATE OR REPLACE TRIGGER search_shop_insert
AFTER INSERT ON shops FOR EACH ROW
BEGIN
    INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('shop',NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
END;

CREATE OR REPLACE TRIGGER search_shop_update
AFTER UPDATE ON shops FOR EACH ROW
BEGIN
    IF NEW.id <> OLD.id THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('shop',OLD.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
    IF NEW.id <> OLD.id OR NOT (BINARY NEW.name <=> BINARY OLD.name) OR NEW.suspended <> OLD.suspended THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('shop',NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
        INSERT INTO search_sync_jobs(entity_type,entity_id)
        SELECT 'product', p.id FROM products p WHERE p.shop_id IN (OLD.id,NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

CREATE OR REPLACE TRIGGER search_shop_delete
AFTER DELETE ON shops FOR EACH ROW
BEGIN
    INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('shop',OLD.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
END;

CREATE OR REPLACE TRIGGER search_taxonomy_update
AFTER UPDATE ON taxonomy FOR EACH ROW
BEGIN
    IF NEW.id <> OLD.id OR NOT (BINARY NEW.name <=> BINARY OLD.name) THEN
        INSERT INTO search_sync_jobs(entity_type,entity_id)
        SELECT 'product', p.id FROM products p WHERE p.brand_id IN (OLD.id,NEW.id) OR p.category_id IN (OLD.id,NEW.id)
        ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
    END IF;
END;

INSERT INTO search_sync_jobs(entity_type,entity_id)
SELECT 'product',id FROM products WHERE TRUE
ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);

INSERT INTO search_sync_jobs(entity_type,entity_id)
SELECT 'shop',id FROM shops WHERE TRUE
ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
