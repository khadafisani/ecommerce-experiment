SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE members
    ADD COLUMN IF NOT EXISTS active BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS updated_by BIGINT NULL;
SET SESSION system_versioning_alter_history = ERROR;
