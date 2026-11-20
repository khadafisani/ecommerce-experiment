ALTER TABLE recommendation_signals
    ADD INDEX IF NOT EXISTS recommendation_recent_views (kind,seen_at,product_id);

CREATE TABLE IF NOT EXISTS recommendation_guest_views (
    view_hour DATETIME NOT NULL,
    product_id BIGINT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    views BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (view_hour,product_id)
) ENGINE=InnoDB;
