-- Your SQL goes here
alter table spar_signups
add column partner_preference integer references spar_series_members (id)
