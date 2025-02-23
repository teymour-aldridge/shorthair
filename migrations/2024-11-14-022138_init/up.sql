create table if not exists users (
    id integer primary key not null,
    public_id text not null unique,
    username text,
    email text not null,
    email_verified boolean not null,
    password_hash text,
    created_at timestamp not null,
    is_superuser boolean not null
);

create table if not exists magic_links (
    id integer primary key not null,
    -- The code for the login.
    code text not null unique,
    user_id integer not null,
    created_at timestamp not null,
    expires_at timestamp not null,
    already_used boolean not null default 'f',
    foreign key (user_id) references users (id)
);

-- allow multiple users access to the same set of data
create table if not exists groups (
    id integer primary key not null,
    public_id text not null unique,
    name text not null unique,
    website text unique,
    created_at timestamp not null
);

create table if not exists group_members (
    id integer primary key not null,
    group_id integer not null,
    user_id integer not null,
    -- whether the user may carry out (potentially) destructive actions, such
    -- as adding/removing members
    has_signing_power boolean not null,
    -- whether the user is an administrator
    is_admin boolean not null,
    foreign key (user_id) references users (id) on delete cascade,
    foreign key (group_id) references groups (id) on delete cascade
);

create table if not exists emails (
    id integer primary key not null,
    -- the smtp message id (useful for tracking this stuff down)
    message_id text not null unique,
    -- the contents of the to field
    recipients text not null,
    created_at timestamp not null
);

-- ############################################################
-- #                                                          #
-- #                        Internals                         #
-- #                                                          #
-- ############################################################
create table if not exists spar_series (
    id integer primary key not null,
    public_id text not null,
    title text not null,
    description text,
    speakers_per_team integer not null,
    group_id integer not null,
    created_at timestamp not null,
    foreign key (group_id) references groups (id)
);

create table if not exists spar_series_members (
    id integer primary key not null,
    public_id text not null,
    name text not null,
    email text not null,
    spar_series_id integer not null,
    created_at timestamp not null,
    foreign key (spar_series_id) references spar_series (id)
);

-- full text search for the spar series members
create virtual table if not exists spar_series_members_fts using fts5 (
    name,
    email,
    content = 'spar_series_members',
    content_rowid = 'id'
);

create trigger if not exists spar_series_members_ai after insert on spar_series_members begin
insert into
    spar_series_members_fts (rowid, name, email)
values
    (new.id, new.name, new.email);

end;

create trigger if not exists spar_series_members_ad after delete on spar_series_members begin
insert into
    spar_series_members_fts (spar_series_members_fts, rowid, name, email)
values
    ('delete', old.id, old.name, old.email);

end;

create trigger if not exists spar_series_members_au after
update on spar_series_members begin
insert into
    spar_series_members_fts (spar_series_members_fts, rowid, name, email)
values
    ('delete', old.id, old.name, old.email);

insert into
    spar_series_members_fts (rowid, name, email)
values
    (new.id, new.name, new.email);

end;

-- todo: clashes
create table if not exists spars (
    id integer primary key not null,
    public_id text not null,
    start_time timestamp not null,
    is_open boolean not null,
    -- Whether or not to release the draw. Once the draw is released, it is no
    -- longer possible to edit it.
    release_draw boolean not null,
    spar_series_id integer not null,
    created_at timestamp not null,
    foreign key (spar_series_id) references spar_series (id)
);

create table if not exists spar_signups (
    id integer primary key not null,
    public_id text not null,
    member_id integer not null,
    spar_id integer not null,
    as_judge boolean not null,
    as_speaker boolean not null,
    foreign key (member_id) references spar_series_members (id),
    foreign key (spar_id) references spars (id),
    unique (member_id, spar_id)
);

create table if not exists spar_rooms (
    id integer primary key not null,
    public_id text not null,
    spar_id integer not null,
    foreign key (spar_id) references spars (id) on delete cascade
);

create table if not exists spar_adjudicators (
    id integer primary key not null,
    public_id text not null,
    member_id integer not null,
    room_id integer not null,
    -- one of "chair", "panelist", "trainee"
    status text not null,
    unique (member_id, room_id),
    foreign key (member_id) references spar_series_members (id),
    foreign key (room_id) references spar_rooms (id) on delete cascade
);

create table if not exists spar_adjudicator_ballot_links (
    id integer primary key not null,
    public_id text not null,
    link text not null,
    room_id integer not null,
    member_id integer not null,
    created_at timestamp not null,
    expires_at timestamp not null,
    foreign key (room_id) references spar_rooms (id) on delete cascade,
    foreign key (member_id) references spar_series_members (id)
);

-- A single team in a spar.
create table if not exists spar_teams (
    id integer primary key not null,
    public_id text not null,
    room_id integer not null,
    position integer not null,
    unique (room_id, position),
    foreign key (room_id) references spar_rooms (id) on delete cascade
);

-- A speaker on a spar team.
create table if not exists spar_speakers (
    id integer primary key not null,
    public_id text not null,
    member_id integer not null,
    team_id integer not null,
    foreign key (team_id) references spar_teams (id) on delete cascade,
    foreign key (member_id) references spar_series_members (id) on delete cascade,
    -- can't place members on multiple teams in the same spar
    unique (member_id, team_id)
);

-- Adjudicator ballots. More recent ballots over-ride previous ballots.
create table if not exists adjudicator_ballots (
    id integer primary key not null,
    public_id text not null,
    adjudicator_id integer not null,
    room_id integer not null,
    created_at timestamp not null,
    foreign key (adjudicator_id) references spar_adjudicators (id) on delete cascade,
    foreign key (room_id) references spar_rooms (id) on delete cascade
);

-- Data actually stored in the ballots
create table if not exists adjudicator_ballot_entries (
    id integer primary key not null,
    public_id text not null,
    ballot_id integer not null,
    speaker_id integer not null,
    team_id integer not null,
    -- todo: some formats require non-integer scores (e.g. Australs)
    speak integer not null,
    -- in increasing order, so for BP this would be
    --
    -- 0 -> PM
    -- 1 -> DPM
    --
    -- whereas for australs (which is not yet supported)
    -- 0 -> Prop1
    -- 1 -> Prop2
    -- 2 -> Prop3
    -- 3 -> Reply
    position integer not null,
    foreign key (speaker_id) references spar_speakers (id) on delete cascade,
    foreign key (team_id) references spar_teams (id) on delete cascade,
    foreign key (ballot_id) references adjudicator_ballots (id) on delete cascade
);
