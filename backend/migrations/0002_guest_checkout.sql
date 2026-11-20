SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE users ADD COLUMN IF NOT EXISTS guest BOOLEAN NOT NULL DEFAULT FALSE;
SET SESSION system_versioning_alter_history = ERROR;
