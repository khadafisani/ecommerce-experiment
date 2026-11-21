CREATE TABLE IF NOT EXISTS user_credentials (
    user_id BIGINT PRIMARY KEY REFERENCES users(id),
    password_hash VARCHAR(255) NOT NULL
) ENGINE=InnoDB;

INSERT INTO user_credentials(user_id,password_hash)
SELECT id,password FROM users WHERE password LIKE '$argon2%'
ON DUPLICATE KEY UPDATE user_id=user_credentials.user_id;

INSERT IGNORE INTO users(email,name,password,suspended)
VALUES ('checkout-expiry@system.tosca.invalid','Automatic checkout expiry','!',TRUE);

SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE checkouts ADD COLUMN IF NOT EXISTS stock_reserved BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE variants MODIFY COLUMN stock BIGINT NOT NULL CHECK(stock >= 0);
SET SESSION system_versioning_alter_history = DEFAULT;

-- Fixture purchases were inserted directly and never reduced inventory.
UPDATE checkouts c JOIN users u ON u.id=c.user_id
SET c.stock_reserved=FALSE
WHERE u.email LIKE '%@seed.tosca.invalid'
  AND NOT EXISTS (SELECT 1 FROM idempotency i
                  WHERE i.actor_id=c.user_id AND i.scope='checkout' AND i.result_id=c.id);

ALTER TABLE checkouts ADD INDEX IF NOT EXISTS checkout_payment_date (payment,created_at,id);
ALTER TABLE login_limits ADD INDEX IF NOT EXISTS login_limit_window (window_start);
ALTER TABLE password_resets ADD INDEX IF NOT EXISTS password_reset_expiry (expires_at);
