-- This file should undo anything in `up.sql`

ALTER TABLE account_invites DROP COLUMN may_create_resources;
