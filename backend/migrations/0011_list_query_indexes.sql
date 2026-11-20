SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE products ADD INDEX IF NOT EXISTS product_shop_status_id (shop_id,status,id);
ALTER TABLE orders ADD INDEX IF NOT EXISTS order_shop_status_date (shop_id,status,created_at,id);
SET SESSION system_versioning_alter_history = ERROR;
ALTER TABLE sessions ADD INDEX IF NOT EXISTS session_created_user (created_at,user_id);
