SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE checkouts ADD INDEX IF NOT EXISTS checkout_user_date (user_id,created_at,id);
ALTER TABLE orders ADD INDEX IF NOT EXISTS order_shop_date (shop_id,created_at,id);
SET SESSION system_versioning_alter_history = ERROR;
