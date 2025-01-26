// @generated automatically by Diesel CLI.

diesel::table! {
    adjudicator_ballot_submissions (id) {
        id -> BigInt,
        public_id -> Text,
        adjudicator_id -> BigInt,
        room_id -> BigInt,
        created_at -> Timestamp,
        ballot_data -> Text,
    }
}

diesel::table! {
    emails (id) {
        id -> BigInt,
        message_id -> Text,
        recipients -> Text,
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
    spar_room_adjudicator (id) {
        id -> BigInt,
        public_id -> Text,
        user_id -> BigInt,
        room_id -> BigInt,
        status -> Text,
    }
}

diesel::table! {
    spar_room_team_speaker (id) {
        id -> BigInt,
        public_id -> Text,
        user_id -> BigInt,
        team_id -> BigInt,
    }
}

diesel::table! {
    spar_room_teams (id) {
        id -> BigInt,
        public_id -> Text,
        room_id -> BigInt,
        position -> BigInt,
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
    }
}

diesel::table! {
    spar_signups (id) {
        id -> BigInt,
        public_id -> Text,
        user_id -> BigInt,
        spar_id -> BigInt,
        as_judge -> Bool,
        as_speaker -> Bool,
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
        password_hash -> Nullable<Text>,
        created_at -> Timestamp,
        is_superuser -> Bool,
    }
}

diesel::joinable!(adjudicator_ballot_submissions -> spar_rooms (room_id));
diesel::joinable!(group_members -> groups (group_id));
diesel::joinable!(group_members -> users (user_id));
diesel::joinable!(magic_links -> users (user_id));
diesel::joinable!(spar_room_adjudicator -> spar_rooms (room_id));
diesel::joinable!(spar_room_adjudicator -> users (user_id));
diesel::joinable!(spar_room_team_speaker -> spar_room_teams (team_id));
diesel::joinable!(spar_room_team_speaker -> users (user_id));
diesel::joinable!(spar_room_teams -> spar_rooms (room_id));
diesel::joinable!(spar_rooms -> spars (spar_id));
diesel::joinable!(spar_series -> groups (group_id));
diesel::joinable!(spar_signups -> spars (spar_id));
diesel::joinable!(spar_signups -> users (user_id));
diesel::joinable!(spars -> spar_series (spar_series_id));

diesel::allow_tables_to_appear_in_same_query!(
    adjudicator_ballot_submissions,
    emails,
    group_members,
    groups,
    magic_links,
    spar_room_adjudicator,
    spar_room_team_speaker,
    spar_room_teams,
    spar_rooms,
    spar_series,
    spar_signups,
    spars,
    users,
);
