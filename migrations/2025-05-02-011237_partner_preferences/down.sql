-- This file should undo anything in `up.sql`
alter table spar_signups
drop column partner_preference;
