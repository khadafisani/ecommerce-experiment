CREATE TABLE recommendation_signals (
    user_id BIGINT NOT NULL REFERENCES users(id),
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    kind ENUM('view','cart') NOT NULL,
    signal_day DATE NOT NULL,
    seen_at DATETIME NOT NULL,
    PRIMARY KEY (user_id,signal_day,product_id,kind),
    INDEX recommendation_signal_expiry (signal_day)
) ENGINE=InnoDB;

CREATE TABLE product_embeddings (
    product_id BIGINT PRIMARY KEY REFERENCES products(id) ON DELETE CASCADE,
    model VARCHAR(60) NOT NULL,
    fingerprint CHAR(64) NOT NULL,
    embedding JSON,
    next_attempt_at DATETIME(6),
    INDEX embedding_due (next_attempt_at,product_id)
) ENGINE=InnoDB;

CREATE VIEW recommendation_product_text AS
SELECT p.id,
    CONCAT('passage: ',p.name,' | ',COALESCE(b.name,''),' | ',COALESCE(c.name,''),' | ',p.description) AS passage
FROM products p JOIN shops s ON s.id=p.shop_id
LEFT JOIN taxonomy b ON b.id=p.brand_id
LEFT JOIN taxonomy c ON c.id=p.category_id
WHERE p.status='active' AND NOT s.suspended;

INSERT INTO search_sync_jobs(entity_type,entity_id)
SELECT 'product',id FROM recommendation_product_text WHERE TRUE
ON DUPLICATE KEY UPDATE revision=revision+1, updated_at=CURRENT_TIMESTAMP(6);
