// @generated automatically by Diesel CLI.

diesel::table! {
    account_invites (id) {
        id -> BigInt,
        public_id -> Text,
        code -> Text,
        email -> Text,
        sent_by -> BigInt,
        created_at -> Timestamp,
        may_create_resources -> Bool,
    }
}

diesel::table! {
    adjudicator_ballot_entries (id) {
        id -> BigInt,
        public_id -> Text,
        ballot_id -> BigInt,
        speaker_id -> BigInt,
        team_id -> BigInt,
        speak -> BigInt,
        position -> BigInt,
    }
}

diesel::table! {
    adjudicator_ballots (id) {
        id -> BigInt,
        public_id -> Text,
        adjudicator_id -> BigInt,
        room_id -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::table! {
    config (id) {
        id -> BigInt,
        public_id -> Text,
        key -> Text,
        value -> Text,
    }
}

diesel::table! {
    draft_draws (id) {
        id -> BigInt,
        public_id -> Text,
        data -> Nullable<Text>,
        spar_id -> BigInt,
        version -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::table! {
    emails (id) {
        id -> BigInt,
        message_id -> Text,
        recipients -> Text,
        contents -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    group_members (id) {
        id -> BigInt,
        group_id -> BigInt,
        user_id -> BigInt,
        has_signing_power -> Bool,
        is_admin -> Bool,
    }
}

diesel::table! {
    groups (id) {
        id -> BigInt,
        public_id -> Text,
        name -> Text,
        website -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    magic_links (id) {
        id -> BigInt,
        code -> Text,
        user_id -> BigInt,
        created_at -> Timestamp,
        expires_at -> Timestamp,
        already_used -> Bool,
    }
}

diesel::table! {
    spar_adjudicator_ballot_links (id) {
        id -> BigInt,
        public_id -> Text,
        link -> Text,
        room_id -> BigInt,
        member_id -> BigInt,
        created_at -> Timestamp,
        expires_at -> Timestamp,
    }
}

diesel::table! {
    spar_adjudicators (id) {
        id -> BigInt,
        public_id -> Text,
        member_id -> BigInt,
        room_id -> BigInt,
        status -> Text,
    }
}

diesel::table! {
    spar_rooms (id) {
        id -> BigInt,
        public_id -> Text,
        spar_id -> BigInt,
    }
}

diesel::table! {
    spar_series (id) {
        id -> BigInt,
        public_id -> Text,
        title -> Text,
        description -> Nullable<Text>,
        speakers_per_team -> BigInt,
        group_id -> BigInt,
        created_at -> Timestamp,
        allow_join_requests -> Bool,
        auto_approve_join_requests -> Bool,
    }
}

diesel::table! {
    spar_series_join_requests (id) {
        id -> BigInt,
        public_id -> Text,
        name -> Text,
        email -> Text,
        spar_series_id -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::table! {
    spar_series_members (id) {
        id -> BigInt,
        public_id -> Text,
        name -> Text,
        email -> Text,
        spar_series_id -> BigInt,
        created_at -> Timestamp,
    }
}

diesel::table! {
    spar_signups (id) {
        id -> BigInt,
        public_id -> Text,
        member_id -> BigInt,
        spar_id -> BigInt,
        as_judge -> Bool,
        as_speaker -> Bool,
        partner_preference -> Nullable<BigInt>,
    }
}

diesel::table! {
    spar_speakers (id) {
        id -> BigInt,
        public_id -> Text,
        member_id -> BigInt,
        team_id -> BigInt,
    }
}

diesel::table! {
    spar_teams (id) {
        id -> BigInt,
        public_id -> Text,
        room_id -> BigInt,
        position -> BigInt,
    }
}

diesel::table! {
    spars (id) {
        id -> BigInt,
        public_id -> Text,
        start_time -> Timestamp,
        is_open -> Bool,
        release_draw -> Bool,
        spar_series_id -> BigInt,
        is_complete -> Bool,
        created_at -> Timestamp,
    }
}

diesel::table! {
    users (id) {
        id -> BigInt,
        public_id -> Text,
        username -> Nullable<Text>,
        email -> Text,
        email_verified -> Bool,
        password_hash -> Text,
        created_at -> Timestamp,
        is_superuser -> Bool,
        may_create_resources -> Bool,
    }
}

diesel::joinable!(account_invites -> users (sent_by));
diesel::joinable!(adjudicator_ballot_entries -> adjudicator_ballots (ballot_id));
diesel::joinable!(adjudicator_ballot_entries -> spar_speakers (speaker_id));
diesel::joinable!(adjudicator_ballot_entries -> spar_teams (team_id));
diesel::joinable!(adjudicator_ballots -> spar_adjudicators (adjudicator_id));
diesel::joinable!(adjudicator_ballots -> spar_rooms (room_id));
diesel::joinable!(draft_draws -> spars (spar_id));
diesel::joinable!(group_members -> groups (group_id));
diesel::joinable!(group_members -> users (user_id));
diesel::joinable!(magic_links -> users (user_id));
diesel::joinable!(spar_adjudicator_ballot_links -> spar_rooms (room_id));
diesel::joinable!(spar_adjudicator_ballot_links -> spar_series_members (member_id));
diesel::joinable!(spar_adjudicators -> spar_rooms (room_id));
diesel::joinable!(spar_adjudicators -> spar_series_members (member_id));
diesel::joinable!(spar_rooms -> spars (spar_id));
diesel::joinable!(spar_series -> groups (group_id));
diesel::joinable!(spar_series_join_requests -> spar_series (spar_series_id));
diesel::joinable!(spar_series_members -> spar_series (spar_series_id));
diesel::joinable!(spar_signups -> spars (spar_id));
diesel::joinable!(spar_speakers -> spar_series_members (member_id));
diesel::joinable!(spar_speakers -> spar_teams (team_id));
diesel::joinable!(spar_teams -> spar_rooms (room_id));
diesel::joinable!(spars -> spar_series (spar_series_id));

diesel::allow_tables_to_appear_in_same_query!(
    account_invites,
    adjudicator_ballot_entries,
    adjudicator_ballots,
    config,
    draft_draws,
    emails,
    group_members,
    groups,
    magic_links,
    spar_adjudicator_ballot_links,
    spar_adjudicators,
    spar_rooms,
    spar_series,
    spar_series_join_requests,
    spar_series_members,
    spar_signups,
    spar_speakers,
    spar_teams,
    spars,
    users,
);
