-- Active hashes already live in user_credentials. An excluded column can still
-- be copied into history when a versioned field changes, so this column can
-- contain only the placeholder. Dropping it first removes prior hash values.
SET SESSION system_versioning_alter_history = KEEP;
ALTER TABLE users DROP COLUMN IF EXISTS password;
ALTER TABLE users
    ADD COLUMN password VARCHAR(1) NOT NULL DEFAULT '!' WITHOUT SYSTEM VERSIONING
    CHECK (password = '!');
SET SESSION system_versioning_alter_history = DEFAULT;
