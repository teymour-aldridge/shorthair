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

-- todo: rename this to spar
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
    user_id integer not null,
    spar_id integer not null,
    as_judge boolean not null,
    as_speaker boolean not null,
    foreign key (user_id) references users (id),
    foreign key (spar_id) references spars (id)
);

create table if not exists spar_rooms (
    id integer primary key not null,
    public_id text not null,
    spar_id integer not null,
    foreign key (spar_id) references spars (id) on delete cascade
);

create table if not exists spar_room_adjudicator (
    id integer primary key not null,
    public_id text not null,
    user_id integer not null,
    room_id integer not null,
    -- one of "chair", "panelist", "trainee"
    status text not null,
    unique (user_id, room_id),
    foreign key (user_id) references users (id) on delete cascade,
    foreign key (room_id) references spar_rooms (id) on delete cascade
);

create table if not exists spar_room_teams (
    id integer primary key not null,
    public_id text not null,
    room_id integer not null,
    position integer not null,
    unique (room_id, position),
    foreign key (room_id) references spar_rooms (id) on delete cascade
);

-- a speaker on a spar team
create table if not exists spar_room_team_speaker (
    id integer primary key not null,
    public_id text not null,
    user_id integer not null,
    team_id integer not null,
    foreign key (team_id) references spar_room_teams (id) on delete cascade,
    foreign key (user_id) references users (id) on delete cascade,
    -- can't place speakers on multiple teams
    unique (user_id, team_id)
);

create table if not exists adjudicator_ballot_submissions (
    id integer primary key not null,
    public_id text not null,
    adjudicator_id integer not null,
    room_id integer not null,
    created_at timestamp not null,
    -- store the ballot as JSON
    -- {og: {s1: 53, s2: 56}, oo: {s1: 56, s2: 67}, cg: {s1: 86, s2: 85}, co: {s1: 82, s2: 83}}
    ballot_data text not null,
    foreign key (adjudicator_id) references spar_room_adjudicators (id),
    foreign key (room_id) references spar_rooms (id)
);
