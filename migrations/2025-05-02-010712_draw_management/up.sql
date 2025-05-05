-- Your SQL goes here
create table if not exists draft_draws (
    id integer primary key not null,
    public_id text not null unique,
    -- we dump the rooms JSON here (this is fine because this data is
    -- fundamentally transient, and only used before the draw for a spar is
    -- generated)
    data text,
    spar_id integer not null,
    version integer not null,
    created_at timestamp not null,
    foreign key (spar_id) references spars (id),
    -- version should be unique for each draft
    unique (id, version)
);
