-- Composite prefixes for category pages and completed-order report ranges.
ALTER TABLE products ADD INDEX IF NOT EXISTS product_category_status_id (category_id,status,id);
ALTER TABLE orders ADD INDEX IF NOT EXISTS order_completed_date (completed_at),
    ADD INDEX IF NOT EXISTS order_shop_completed_date (shop_id,completed_at);
