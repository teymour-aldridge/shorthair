use std::collections::{HashMap, HashSet};

use arbitrary::Arbitrary;
use chrono::Utc;
use db::{
    ballot::{
        AdjudicatorBallot, AdjudicatorBallotEntry, AdjudicatorBallotLink,
    },
    group::{Group, GroupMember},
    schema::users,
    spar::{
        Spar, SparRoom, SparRoomAdjudicator, SparRoomTeam, SparRoomTeamSpeaker,
        SparSeries, SparSeriesMember, SparSignup,
    },
    user::User,
};
use diesel::OptionalExtension;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use fuzzcheck::DefaultMutator;
use fuzzcheck_util::usize_u64_mutator::{
    usize_within_range_mutator, UsizeWithinRangeMutator,
};
use rocket::{http::ContentType, local::blocking::Client, Build, Rocket};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    admin::setup::SetupForm,
    auth::login::PasswordLoginForm,
    groups::{CreateGroupForm, CreateSparSeriesForm},
    spar_generation::{
        ballots::BpBallotForm,
        individual_spars::signup_routes::SignupForSpar,
        spar_series::admin_routes::{AddMemberForm, MakeSessionForm},
    },
};

use super::sync::id::last_id;

#[derive(Debug, Clone, Eq, PartialEq, Hash, arbitrary::Arbitrary)]
pub struct GroupMembershipData {
    is_admin: bool,
    is_superuser: bool,
}

/// The state of the model.
///
/// Note: we adjust the `id` field of each model to store its index in the field
/// it represents.
#[derive(Debug)]
pub struct State {
    pub client: rocket::local::blocking::Client,
    users: Vec<User>,
    groups: Vec<Group>,
    group_members: HashMap<(usize, Group), GroupMembershipData>,
    spar_series: Vec<SparSeries>,
    spar_series_members: Vec<SparSeriesMember>,
    spars: Vec<Spar>,
    spar_signups: Vec<SparSignup>,
    active_user: Option<User>,
    rooms: Vec<SparRoom>,
    teams: Vec<SparRoomTeam>,
    adjs: Vec<SparRoomAdjudicator>,
    speakers: Vec<SparRoomTeamSpeaker>,
    ballots: Vec<AdjudicatorBallot>,
    ballot_links: Vec<AdjudicatorBallotLink>,
    ballot_entries: Vec<AdjudicatorBallotEntry>,
}

impl State {
    pub fn reset(&mut self) {
        self.active_user = None;
        self.users.clear();
        self.groups.clear();
        self.group_members.clear();
        self.spar_series.clear();
        self.spar_series_members.clear();
        self.spars.clear();
        self.spar_signups.clear();
        self.rooms.clear();
        self.teams.clear();
        self.adjs.clear();
        self.speakers.clear();
        self.ballots.clear();
        self.ballot_links.clear();
        self.ballot_entries.clear();
    }

    /// Delete all rows in the database.
    pub fn reset_db(
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Result<(), diesel::result::Error> {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            diesel::delete(db::schema::adjudicator_ballots::table)
                .execute(conn)?;
            diesel::delete(db::schema::adjudicator_ballot_entries::table)
                .execute(conn)?;
            diesel::delete(db::schema::spar_adjudicator_ballot_links::table)
                .execute(conn)?;
            diesel::delete(db::schema::spar_speakers::table).execute(conn)?;
            diesel::delete(db::schema::spar_adjudicators::table)
                .execute(conn)?;
            diesel::delete(db::schema::spar_speakers::table).execute(conn)?;
            diesel::delete(db::schema::spar_teams::table).execute(conn)?;
            diesel::delete(db::schema::spar_rooms::table).execute(conn)?;
            diesel::delete(db::schema::spar_signups::table).execute(conn)?;
            diesel::delete(db::schema::spars::table).execute(conn)?;
            diesel::delete(db::schema::spar_series_members::table)
                .execute(conn)?;
            diesel::delete(db::schema::spar_series::table).execute(conn)?;
            diesel::delete(db::schema::group_members::table).execute(conn)?;
            diesel::delete(db::schema::groups::table).execute(conn)?;
            diesel::delete(db::schema::users::table).execute(conn)?;
            Ok(())
        })
    }

    /// Creates a new empty model given the provided Rocket client.
    pub fn of_rocket(rocket: Rocket<Build>) -> Self {
        let client = Client::tracked(rocket).unwrap();
        Self {
            client,
            users: Default::default(),
            groups: Default::default(),
            group_members: Default::default(),
            spar_series: Default::default(),
            spars: Default::default(),
            spar_signups: Default::default(),
            active_user: Default::default(),
            rooms: Default::default(),
            teams: Default::default(),
            adjs: Default::default(),
            speakers: Default::default(),
            ballots: Default::default(),
            ballot_links: Default::default(),
            ballot_entries: Default::default(),
            spar_series_members: Default::default(),
        }
    }

    /// Steps through the provided actions. The application is always run first
    /// (as the model relies on the application in order to generate
    /// synchronized identifiers).
    pub fn run(
        &mut self,
        actions: &[Action],
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        for action in actions {
            self.step_app(action);
            self.step_model(action, conn);
            self.assert_matches_database(conn);
            // todo: make a copy and assert that the sync doesn't change
            // anything?
            // self.sync(conn);
        }
    }

    /// Checks whether the state of the application matches the state of the
    /// model.
    ///
    /// TODO: might speed up the fuzzer if we return a Result (with detailed
    /// error information) rather than panicking here.
    pub fn assert_matches_database(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            self.check_users(conn);
            self.check_groups(conn);
            self.check_spar_series(conn);
            self.check_spars(conn);
            self.check_spar_signups(conn);
            self.check_spar_series_members(conn);
            self.check_rooms(conn);
            self.check_teams(conn);
            self.check_adjs(conn);
            self.check_speakers(conn);
            self.check_ballots(conn);
            Ok(())
        })
        .unwrap()
    }

    fn check_ballots(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let ballot_count = db::schema::adjudicator_ballots::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(ballot_count as usize, self.ballots.len());
        for ballot in &self.ballots {
            let db_ballot = db::schema::adjudicator_ballots::table
                .filter(
                    db::schema::adjudicator_ballots::public_id
                        .eq(&ballot.public_id),
                )
                .first::<AdjudicatorBallot>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for ballot {:?}", ballot));
            assert_eq!(ballot.public_id, db_ballot.public_id);
            assert_eq!(ballot.adjudicator_id, db_ballot.adjudicator_id);
            assert_eq!(ballot.room_id, db_ballot.room_id);
        }
    }

    fn check_speakers(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let speaker_count = db::schema::spar_speakers::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(speaker_count as usize, self.speakers.len());
        for speaker in &self.speakers {
            let db_speaker = db::schema::spar_speakers::table
                .filter(
                    db::schema::spar_speakers::public_id.eq(&speaker.public_id),
                )
                .first::<SparRoomTeamSpeaker>(conn)
                .optional()
                .unwrap()
                .expect(&format!(
                    "No matching record for speaker {:?}",
                    speaker
                ));
            assert_eq!(speaker.public_id, db_speaker.public_id);
            assert_eq!(speaker.member_id, db_speaker.member_id);
            assert_eq!(speaker.team_id, db_speaker.team_id);
        }
    }

    fn check_adjs(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let adj_count = db::schema::spar_adjudicators::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(adj_count as usize, self.adjs.len());
        for adj in &self.adjs {
            let db_adj = db::schema::spar_adjudicators::table
                .filter(
                    db::schema::spar_adjudicators::public_id.eq(&adj.public_id),
                )
                .first::<SparRoomAdjudicator>(conn)
                .optional()
                .unwrap()
                .expect(&format!(
                    "No matching record for adjudicator {:?}",
                    adj
                ));
            assert_eq!(adj.public_id, db_adj.public_id);
            assert_eq!(adj.member_id, db_adj.member_id);
            assert_eq!(adj.room_id, db_adj.room_id);
            assert_eq!(adj.status, db_adj.status);
        }
    }

    fn check_teams(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let team_count = db::schema::spar_teams::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(team_count as usize, self.teams.len());
        for team in &self.teams {
            let db_team = db::schema::spar_teams::table
                .filter(db::schema::spar_teams::public_id.eq(&team.public_id))
                .first::<SparRoomTeam>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for team {:?}", team));
            assert_eq!(team.public_id, db_team.public_id);
            assert_eq!(team.position, db_team.position);
        }
    }

    fn check_rooms(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let room_count = db::schema::spar_rooms::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(room_count as usize, self.rooms.len());
        for room in &self.rooms {
            let db_room = db::schema::spar_rooms::table
                .filter(db::schema::spar_rooms::public_id.eq(&room.public_id))
                .first::<SparRoom>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for room {:?}", room));
            assert_eq!(room.public_id, db_room.public_id);
        }
    }

    fn check_spar_series_members(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let member_count = db::schema::spar_series_members::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(member_count as usize, self.spar_series_members.len());
        for member in &self.spar_series_members {
            let db_member = db::schema::spar_series_members::table
                .filter(
                    db::schema::spar_series_members::public_id
                        .eq(&member.public_id),
                )
                .first::<SparSeriesMember>(conn)
                .optional()
                .unwrap()
                .expect(&format!(
                    "No matching record for series member {:?}",
                    member
                ));
            assert_eq!(member.public_id, db_member.public_id);
            assert_eq!(member.name, db_member.name);
            assert_eq!(member.email, db_member.email);
        }
    }

    fn check_spar_signups(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let signup_count = db::schema::spar_signups::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(signup_count as usize, self.spar_signups.len());
        for signup in &self.spar_signups {
            let db_signup = db::schema::spar_signups::table
                .filter(
                    db::schema::spar_signups::public_id.eq(&signup.public_id),
                )
                .first::<SparSignup>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for signup {:?}", signup));
            assert_eq!(signup.as_judge, db_signup.as_judge);
            assert_eq!(signup.as_speaker, db_signup.as_speaker);
            assert_eq!(signup.public_id, db_signup.public_id);
        }
    }

    fn check_spars(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let spar_count = db::schema::spars::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(spar_count as usize, self.spars.len());
        for spar in &self.spars {
            let db_spar = db::schema::spars::table
                .filter(db::schema::spars::public_id.eq(&spar.public_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for spar {:?}", spar));
            assert_eq!(spar.is_open, db_spar.is_open);
            assert_eq!(spar.public_id, db_spar.public_id);
        }
    }

    fn check_spar_series(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let series_count = db::schema::spar_series::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(series_count as usize, self.spar_series.len());
        for series in &self.spar_series {
            let db_series = db::schema::spar_series::table
                .filter(
                    db::schema::spar_series::public_id.eq(&series.public_id),
                )
                .first::<SparSeries>(conn)
                .optional()
                .unwrap()
                .expect(&format!(
                    "No matching record for spar series {:?}",
                    series
                ));
            assert_eq!(series.title, db_series.title);
            assert_eq!(series.description, db_series.description);
            assert_eq!(series.public_id, db_series.public_id);
        }
    }

    fn check_groups(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let group_count = db::schema::groups::table
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(group_count as usize, self.groups.len());
        for group in &self.groups {
            let db_group = db::schema::groups::table
                .filter(db::schema::groups::public_id.eq(&group.public_id))
                .first::<Group>(conn)
                .optional()
                .unwrap()
                .expect(&format!("No matching record for group {:?}", group));
            assert_eq!(group.name, db_group.name);
            assert_eq!(group.website, db_group.website);
            assert_eq!(group.public_id, db_group.public_id);
        }
    }

    fn check_users(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let count = users::table.count().get_result::<i64>(conn).unwrap();
        assert_eq!(count as usize, self.users.len());
        for user in &self.users {
            let db_user = users::table
                .filter(users::public_id.eq(&user.public_id))
                .first::<User>(conn)
                .optional()
                .unwrap()
                .expect(&format!(
                    "error: no matching record for user {:?}",
                    user
                ));
            assert_eq!(user.username, db_user.username);
            assert_eq!(user.email_verified, db_user.email_verified);
            assert_eq!(user.email, db_user.email);
        }
    }

    /// Removes all data in the model, and then creates a copy of all the data
    /// in the database in the model.
    ///
    /// We do this mostly for the spar draw generation functionality, as this is
    /// not tested by the model (it is has some manual tests).
    fn sync(
        &mut self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        let active_user =
            self.active_user.as_ref().map(|user| user.public_id.clone());
        let user_passwords = self
            .users
            .iter()
            .map(|user| (user.public_id.clone(), user.password_hash.clone()))
            .collect::<HashMap<_, _>>();
        self.reset();
        let mut user_id_map = HashMap::new();
        let mut group_id_map = HashMap::new();
        let mut series_id_map = HashMap::new();
        let mut member_id_map = HashMap::new();
        let mut spar_id_map = HashMap::new();
        let mut room_id_map = HashMap::new();
        let mut team_id_map = HashMap::new();
        let mut adj_id_map = HashMap::new();
        let mut speaker_id_map = HashMap::new();

        self.users = users::table.load::<User>(conn).unwrap();
        for i in 0..self.users.len() {
            user_id_map.insert(self.users[i].id, i);
            self.users[i].id = i as i64;
            self.users[i].password_hash = user_passwords
                .get(&self.users[i].public_id)
                .unwrap()
                .clone();
        }

        if let Some(public_id) = active_user {
            self.active_user = Some(
                self.users
                    .iter()
                    .find(|user| user.public_id == public_id)
                    .cloned()
                    .unwrap(),
            );
        }

        self.groups = db::schema::groups::table.load::<Group>(conn).unwrap();
        for i in 0..self.groups.len() {
            group_id_map.insert(self.groups[i].id, i);
            self.groups[i].id = i as i64;
        }

        let group_members = db::schema::group_members::table
            .load::<GroupMember>(conn)
            .unwrap();
        for member in group_members {
            let user_id = user_id_map[&member.user_id];
            let user = self.users[user_id].clone();
            let group_id = group_id_map[&member.group_id];
            self.group_members.insert(
                (user.id as usize, self.groups[group_id].clone()),
                GroupMembershipData {
                    is_admin: member.is_admin,
                    is_superuser: member.has_signing_power,
                },
            );
        }

        self.spar_series = db::schema::spar_series::table
            .load::<SparSeries>(conn)
            .unwrap();
        for i in 0..self.spar_series.len() {
            series_id_map.insert(self.spar_series[i].id, i);
            self.spar_series[i].id = i as i64;
            self.spar_series[i].group_id =
                group_id_map[&self.spar_series[i].group_id] as i64;
        }

        self.spar_series_members = db::schema::spar_series_members::table
            .load::<SparSeriesMember>(conn)
            .unwrap();

        for i in 0..self.spar_series_members.len() {
            member_id_map.insert(self.spar_series_members[i].id, i);
            self.spar_series_members[i].id = i as i64;
            self.spar_series_members[i].spar_series_id = series_id_map
                [&self.spar_series_members[i].spar_series_id]
                as i64;
        }

        self.spars = db::schema::spars::table.load::<Spar>(conn).unwrap();
        for i in 0..self.spars.len() {
            spar_id_map.insert(self.spars[i].id, i);
            self.spars[i].id = i as i64;
            self.spars[i].spar_series_id =
                series_id_map[&self.spars[i].spar_series_id] as i64;
        }

        self.spar_signups = db::schema::spar_signups::table
            .load::<SparSignup>(conn)
            .unwrap();
        for i in 0..self.spar_signups.len() {
            self.spar_signups[i].id = i as i64;
            self.spar_signups[i].member_id =
                member_id_map[&self.spar_signups[i].member_id] as i64;
            self.spar_signups[i].spar_id =
                spar_id_map[&self.spar_signups[i].spar_id] as i64;
        }

        self.rooms = db::schema::spar_rooms::table
            .load::<SparRoom>(conn)
            .unwrap();
        for i in 0..self.rooms.len() {
            room_id_map.insert(self.rooms[i].id, i);
            self.rooms[i].id = i as i64;
            self.rooms[i].spar_id = spar_id_map[&self.rooms[i].spar_id] as i64;
        }

        self.teams = db::schema::spar_teams::table
            .load::<SparRoomTeam>(conn)
            .unwrap();
        for i in 0..self.teams.len() {
            team_id_map.insert(self.teams[i].id, i);
            self.teams[i].id = i as i64;
            self.teams[i].room_id = room_id_map[&self.teams[i].room_id] as i64;
        }

        self.adjs = db::schema::spar_adjudicators::table
            .load::<SparRoomAdjudicator>(conn)
            .unwrap();
        for i in 0..self.adjs.len() {
            adj_id_map.insert(self.adjs[i].id, i);
            self.adjs[i].id = i as i64;
            self.adjs[i].member_id =
                user_id_map[&self.adjs[i].member_id] as i64;
            self.adjs[i].room_id = room_id_map[&self.adjs[i].room_id] as i64;
        }

        self.speakers = db::schema::spar_speakers::table
            .load::<SparRoomTeamSpeaker>(conn)
            .unwrap();
        for i in 0..self.speakers.len() {
            speaker_id_map.insert(self.speakers[i].id, i);
            self.speakers[i].id = i as i64;
            self.speakers[i].member_id =
                user_id_map[&self.speakers[i].member_id] as i64;
            self.speakers[i].team_id =
                team_id_map[&self.speakers[i].team_id] as i64;
        }

        self.ballots = db::schema::adjudicator_ballots::table
            .load::<AdjudicatorBallot>(conn)
            .unwrap();
        for i in 0..self.ballots.len() {
            self.ballots[i].id = i as i64;
            self.ballots[i].adjudicator_id =
                adj_id_map[&self.ballots[i].adjudicator_id] as i64;
            self.ballots[i].room_id =
                room_id_map[&self.ballots[i].room_id] as i64;
        }
    }

    /// Do a single state transition.
    pub fn step(
        &mut self,
        action: &Action,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        self.step_model(action, conn);
        self.step_app(action);
        self.assert_matches_database(conn);
    }

    /// Apply action to the model.
    fn step_model(
        &mut self,
        action: &Action,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) {
        match action {
            Action::Setup(user) => {
                if !self.users.is_empty() {
                    return;
                }

                if user.username.is_none() {
                    return;
                }

                if !User::validate_password(&user.password_hash) {
                    return;
                }

                if !User::validate_email(&user.email) {
                    return;
                }

                if !User::validate_username(user.username.as_ref().unwrap()) {
                    return;
                }

                let mut user = user.clone();
                // note: we want IDs to line up with the indices in the
                // relevant places
                user.id = 0;
                user.email_verified = false;
                user.is_superuser = true;
                user.public_id = last_id().unwrap().to_string();
                self.users.push(user);
            }
            Action::Login(user) => {
                if !self.users.is_empty() {
                    let clamped =
                        (*user).clamp(0, self.users.len().saturating_sub(1));
                    let user = &self.users[clamped];
                    self.active_user = Some(user.clone());
                }
            }
            Action::CreateGroup(group) => {
                if self.active_user.is_none() {
                    return;
                }

                let user = self.active_user.as_ref().unwrap();
                let mut group = group.clone();

                if !Group::validate_name(&group.name) {
                    return;
                }

                if self.groups.iter().any(|t| {
                    t.name == group.name
                        || (group.website.is_some()
                            && group.website == t.website)
                }) {
                    return;
                }

                let n = self.groups.len();
                group.id = n as i64;
                group.public_id = last_id().unwrap().to_string();
                self.groups.push(group);
                assert_eq!(self.groups[n].id as usize, n);
                self.group_members.insert(
                    (user.id as usize, self.groups[n].clone()),
                    GroupMembershipData {
                        is_admin: true,
                        is_superuser: true,
                    },
                );
            }
            Action::CreateSparSeries(spar_series) => {
                let user = if let Some(user) = &self.active_user {
                    user
                } else {
                    return;
                };

                let group_idx = (spar_series.group_id as usize)
                    .clamp(0, self.groups.len().saturating_sub(1));
                let group = if let Some(group) = self.groups.get(group_idx) {
                    group
                } else {
                    return;
                };

                let membership = if let Some(membership) =
                    self.group_members.get(&(user.id as usize, group.clone()))
                {
                    membership
                } else {
                    return;
                };

                if !(membership.is_admin || membership.is_superuser) {
                    return;
                }

                if self.spar_series.iter().any(|s| {
                    s.group_id == group.id && s.title == spar_series.title
                }) {
                    return;
                }

                let spar_series_id = self.spar_series.len();
                let mut spar_series = spar_series.clone();
                spar_series.id = spar_series_id as i64;
                spar_series.group_id = group.id;
                spar_series.public_id = last_id().unwrap().to_string();
                self.spar_series.push(spar_series);
                assert_eq!(
                    self.spar_series[spar_series_id].id as usize,
                    spar_series_id
                );
            }
            Action::CreateSpar(spar) => {
                let user = match &self.active_user {
                    Some(user) => user,
                    None => return,
                };
                let series_idx = (spar.spar_series_id as usize)
                    .clamp(0, self.spar_series.len().saturating_sub(1));
                let spar_series = match self.spar_series.get(series_idx) {
                    Some(series) => series,
                    None => return,
                };
                let group = match self.groups.get(spar_series.group_id as usize)
                {
                    Some(group) => group,
                    None => return,
                };
                let membership = match self
                    .group_members
                    .get(&(user.id as usize, group.clone()))
                {
                    Some(membership) => membership,
                    None => return,
                };
                if !(membership.is_admin || membership.is_superuser) {
                    return;
                }
                let all_previous_spars_complete = self
                    .spars
                    .iter()
                    .filter(|spar| spar.spar_series_id as usize == series_idx)
                    // use `fold` instead of `all` in order to ensure that
                    // `[] -> true`
                    .fold(true, |acc, next| acc && next.is_complete);
                if !all_previous_spars_complete {
                    return;
                }
                let spar_id = self.spars.len();
                let mut spar = spar.clone();
                spar.id = spar_id as i64;
                spar.spar_series_id = spar_series.id;
                spar.is_complete = false;
                spar.release_draw = false;
                spar.public_id = last_id().unwrap().to_string();
                self.spars.push(spar);
                assert_eq!(self.spars[spar_id as usize].id as usize, spar_id);
            }
            Action::Signup {
                member_idx,
                spar_idx,
                as_judge,
                as_speaker,
            } => {
                let spar_idx = (*spar_idx)
                    .clamp(0, self.spar_signups.len().saturating_sub(1));
                let member_idx = (*member_idx)
                    .clamp(0, self.spar_series_members.len().saturating_sub(1));

                let spar = self.spars.get(spar_idx);
                if spar.is_none() {
                    return;
                }
                let spar = spar.unwrap();

                let member = self.spar_series_members.get(member_idx);
                if member.is_none() {
                    return;
                }
                let member = member.unwrap();

                if !spar.is_open {
                    return;
                }

                assert!(
                    !spar.release_draw,
                    "error: draw should not be released if the spar is open for signups. Note: spar = {spar:#?}",
                );

                let signup_idx =
                    self.spar_signups.iter().enumerate().find(|(_, signup)| {
                        signup.member_id == member.id
                            && signup.spar_id == spar.id
                    });

                if let Some((idx, _)) = signup_idx {
                    self.spar_signups[idx].as_judge = *as_judge;
                    self.spar_signups[idx].as_speaker = *as_speaker;
                } else {
                    self.spar_signups.push(SparSignup {
                        id: self.spar_signups.len() as i64,
                        public_id: last_id().unwrap().to_string(),
                        member_id: member.id,
                        spar_id: spar.id,
                        as_judge: *as_judge,
                        as_speaker: *as_speaker,
                        // todo: support this
                        partner_preference: None,
                    });
                }
            }
            Action::GenerateDraw(_spar) => self.sync(conn),
            Action::SubmitBallot(ballot, adj_idx, room_idx) => {
                let adj_idx =
                    (*adj_idx).clamp(0, self.adjs.len().saturating_sub(1));
                let room_idx =
                    (*room_idx).clamp(0, self.rooms.len().saturating_sub(1));

                let adj = match self.adjs.get(adj_idx) {
                    Some(adj) => adj,
                    None => return,
                };

                let room = match self.rooms.get(room_idx) {
                    Some(room) => room,
                    None => return,
                };

                let speaker_len = self.speakers.len().saturating_sub(1);

                let speaker_indices = [
                    ballot.pm.clamp(0, speaker_len),
                    ballot.dpm.clamp(0, speaker_len),
                    ballot.lo.clamp(0, speaker_len),
                    ballot.dlo.clamp(0, speaker_len),
                    ballot.mg.clamp(0, speaker_len),
                    ballot.gw.clamp(0, speaker_len),
                    ballot.mo.clamp(0, speaker_len),
                    ballot.ow.clamp(0, speaker_len),
                ];

                let mut speakers = Vec::with_capacity(8);
                for &idx in &speaker_indices {
                    match self.speakers.get(idx) {
                        Some(s) => speakers.push(s),
                        None => return,
                    }
                }

                let [pm, dpm, lo, dlo, mg, gw, mo, ow] =
                    <[&SparRoomTeamSpeaker; 8]>::try_from(speakers).unwrap();

                let find_team = |position| {
                    self.teams.iter().find(|team| {
                        team.room_id == room.id && team.position == position
                    })
                };

                let og = match find_team(0) {
                    Some(t) => t,
                    None => return,
                };
                let oo = match find_team(1) {
                    Some(t) => t,
                    None => return,
                };
                let cg = match find_team(2) {
                    Some(t) => t,
                    None => return,
                };
                let co = match find_team(3) {
                    Some(t) => t,
                    None => return,
                };

                let og_score = ballot.pm_score + ballot.dpm_score;
                let oo_score = ballot.lo_score + ballot.dlo_score;
                let cg_score = ballot.mg_score + ballot.gw_score;
                let co_score = ballot.mo_score + ballot.ow_score;

                let mut scores = HashSet::with_capacity(4);
                scores.insert(og_score);
                scores.insert(oo_score);
                scores.insert(cg_score);
                scores.insert(co_score);

                if mo.team_id != co.id || ow.team_id != co.id {
                    return;
                }

                if mg.team_id != cg.id || gw.team_id != cg.id {
                    return;
                }

                if lo.team_id != oo.id || dlo.team_id != oo.id {
                    return;
                }

                if pm.team_id != og.id || dpm.team_id != og.id {
                    return;
                }

                let ballot_id = self.ballots.len() as i64;
                self.ballots.push(AdjudicatorBallot {
                    id: ballot_id,
                    public_id: last_id().unwrap().to_string(),
                    adjudicator_id: adj.id,
                    room_id: room.id,
                    created_at: Utc::now().naive_utc(),
                });

                let entries = [
                    (pm, ballot.pm_score, og.id, 0),
                    (dpm, ballot.dpm_score, og.id, 1),
                    (lo, ballot.lo_score, oo.id, 0),
                    (dlo, ballot.dlo_score, oo.id, 1),
                    (mg, ballot.mg_score, cg.id, 0),
                    (gw, ballot.gw_score, cg.id, 1),
                    (mo, ballot.mo_score, co.id, 0),
                    (ow, ballot.ow_score, co.id, 1),
                ];

                for (speaker, score, team_id, position) in entries {
                    self.ballot_entries.push(AdjudicatorBallotEntry {
                        id: self.ballot_entries.len() as i64,
                        public_id: Uuid::now_v7().to_string(),
                        ballot_id,
                        speaker_id: speaker.id,
                        team_id,
                        speak: score,
                        position,
                    });
                }
            }
            Action::AddMember(spar_series_member) => {
                if !User::validate_username(&spar_series_member.name)
                    || !User::validate_email(&spar_series_member.email)
                {
                    return;
                }

                let user = match &self.active_user {
                    Some(u) => u,
                    None => return,
                };

                let spar_series_idx = (spar_series_member.spar_series_id
                    as usize)
                    .clamp(0, self.spar_series_members.len().saturating_sub(1));
                let spar_series = match self.spar_series.get(spar_series_idx) {
                    Some(s) => s,
                    None => return,
                };

                let group = &self.groups[spar_series.group_id as usize];

                let membership = match self
                    .group_members
                    .get(&(user.id as usize, group.clone()))
                {
                    Some(m) => m,
                    None => return,
                };

                if !membership.is_admin && !membership.is_superuser {
                    return;
                }

                let mut member = spar_series_member.clone();
                member.spar_series_id = spar_series.id;
                member.id = self.spar_series_members.len() as i64;
                member.public_id = last_id().unwrap().to_string();
                self.spar_series_members.push(member);
            }
            Action::ReleaseDraw(spar_idx) => {
                let user_opt = &self.active_user;
                if user_opt.is_none() {
                    return;
                }
                let user = user_opt.as_ref().unwrap();

                let spar_idx =
                    (*spar_idx).clamp(0, self.spars.len().saturating_sub(1));
                let spar_opt = self.spars.get(spar_idx).clone();
                if spar_opt.is_none() {
                    return;
                }
                let spar = spar_opt.unwrap();

                let series = &self.spar_series[spar.spar_series_id as usize];
                let group = &self.groups[series.group_id as usize];

                let membership_opt =
                    self.group_members.get(&(user.id as usize, group.clone()));
                if membership_opt.is_none() {
                    return;
                }
                let membership = membership_opt.unwrap();

                if !(membership.is_admin || membership.is_superuser) {
                    return;
                }

                self.spars[spar_idx].release_draw = true;
                self.spars[spar_idx].is_open = false;
            }
            Action::SetSparIsOpen { spar, state } => {
                if self.active_user.is_none() {
                    return;
                }

                let user = self.active_user.as_ref().unwrap();
                let spar_idx =
                    (*spar).clamp(0, self.spars.len().saturating_sub(1));

                let spar_opt = self.spars.get(spar_idx);
                if spar_opt.is_none() {
                    return;
                }

                let spar = spar_opt.unwrap();
                let series = &self.spar_series[spar.spar_series_id as usize];
                let group = &self.groups[series.group_id as usize];

                let member_opt =
                    self.group_members.get(&(user.id as usize, group.clone()));
                if member_opt.is_none() {
                    return;
                }

                let member = member_opt.unwrap();
                if !member.is_admin && !member.is_superuser {
                    return;
                }

                self.spars[spar_idx].is_open = *state;
                if *state {
                    self.spars[spar_idx].release_draw = false;
                }
            }
        }
    }

    /// Apply action to the real application.
    fn step_app(&self, action: &Action) {
        match action {
            Action::Setup(user) => {
                if let Some(username) = &user.username {
                    let password = &user.password_hash;
                    self.client
                        .post("/admin/setup")
                        .header(ContentType::Form)
                        .body(
                            serde_urlencoded::to_string(&SetupForm {
                                username: username.clone(),
                                email: user.email.clone(),
                                password: password.clone(),
                                password2: password.clone(),
                            })
                            .unwrap(),
                        )
                        .dispatch();
                }
            }
            Action::Login(user) => {
                let clamped =
                    (*user).clamp(0, self.users.len().saturating_sub(1));
                if let Some(user) = self.users.get(clamped) {
                    let password = &user.password_hash;
                    self.client
                        .post("/login")
                        .header(ContentType::Form)
                        .body(
                            serde_urlencoded::to_string(&PasswordLoginForm {
                                email: user.email.clone(),
                                password: password.clone(),
                            })
                            .unwrap(),
                        )
                        .dispatch();
                }
            }
            Action::CreateGroup(group) => {
                self.client
                    .post("/groups/new")
                    .header(ContentType::Form)
                    .body(
                        serde_urlencoded::to_string(&CreateGroupForm {
                            name: group.name.clone(),
                            website: group.website.clone(),
                        })
                        .unwrap(),
                    )
                    .dispatch();
            }
            Action::CreateSparSeries(spar_series) => {
                // we interpret spar_series.group_id as an index into the group
                // ids
                let group_idx = (spar_series.group_id as usize)
                    .clamp(0, self.groups.len().saturating_sub(1));

                if let Some(group) = self.groups.get(group_idx) {
                    self.client
                        .post(format!(
                            "/groups/{}/spar_series/new",
                            group.public_id
                        ))
                        .header(ContentType::Form)
                        .body(
                            serde_urlencoded::to_string(
                                &CreateSparSeriesForm {
                                    title: spar_series.title.clone(),
                                    description: spar_series
                                        .description
                                        .clone(),
                                },
                            )
                            .unwrap(),
                        )
                        .dispatch();
                }
            }
            Action::CreateSpar(spar) => {
                let series_idx = (spar.spar_series_id as usize)
                    .clamp(0, self.spar_series.len().saturating_sub(1));
                if let Some(spar_series) = self.spar_series.get(series_idx) {
                    self.client
                        .post(format!(
                            "/spar_series/{}/makesess",
                            spar_series.public_id
                        ))
                        .header(ContentType::Form)
                        .body(
                            serde_urlencoded::to_string({
                                &MakeSessionForm {
                                    // todo: proper timezone handling
                                    start_time: spar
                                        .start_time
                                        .format("%Y-%m-%dT%H:%M")
                                        .to_string(),
                                    // todo: remove this field
                                    is_open: if spar.is_open {
                                        Some("true".to_string())
                                    } else {
                                        None
                                    },
                                }
                            })
                            .unwrap(),
                        )
                        .dispatch();
                }
            }
            Action::ReleaseDraw(spar_idx) => {
                let spar_idx =
                    (*spar_idx).clamp(0, self.spars.len().saturating_sub(1));
                if let Some(spar) = self.spars.get(spar_idx).clone() {
                    self.client
                        .post(format!(
                            "/spars/{}/set_released?released=true",
                            spar.public_id
                        ))
                        .dispatch();
                }
            }
            Action::Signup {
                member_idx,
                spar_idx,
                as_judge,
                as_speaker,
            } => {
                let spar_idx = (*spar_idx)
                    .clamp(0, self.spar_signups.len().saturating_sub(1));
                let member_idx = (*member_idx)
                    .clamp(0, self.spar_series_members.len().saturating_sub(1));

                if let Some(spar) = self.spars.get(spar_idx) {
                    if let Some(member) =
                        self.spar_series_members.get(member_idx)
                    {
                        self.client
                            .post(format!(
                                "/spars/{}/signup/{}",
                                spar.public_id, member.public_id
                            ))
                            .header(ContentType::Form)
                            .body(
                                serde_urlencoded::to_string({
                                    &SignupForSpar {
                                        as_judge: *as_judge,
                                        as_speaker: *as_speaker,
                                        speaking_partner: None,
                                    }
                                })
                                .unwrap(),
                            )
                            .dispatch();
                    }
                }
            }
            Action::GenerateDraw(spar_idx) => {
                let spar_idx =
                    (*spar_idx).clamp(0, self.spars.len().saturating_sub(1));
                if let Some(spar) = self.spars.get(spar_idx) {
                    self.client
                        .post(format!("/spars/{}/makedraw", spar.public_id))
                        .dispatch();
                }
            }
            Action::SubmitBallot(ballot, adj_idx, room_idx) => {
                let adj_idx =
                    (*adj_idx).clamp(0, self.adjs.len().saturating_sub(1));
                let room_idx =
                    (*room_idx).clamp(0, self.rooms.len().saturating_sub(1));

                if let Some(adj) = self.adjs.get(adj_idx) {
                    if let Some(room) = self.rooms.get(room_idx) {
                        let key = self
                            .ballot_links
                            .iter()
                            .find(|key| {
                                key.room_id == room.id
                                    && key.member_id == adj.member_id
                            })
                            .unwrap();

                        let resolve_public_id = |idx: &usize| {
                            let idx = (*idx).clamp(
                                0,
                                self.speakers.len().saturating_sub(1),
                            );
                            self.speakers[idx].public_id.clone()
                        };

                        let ballot = BpBallotForm {
                            force: true,
                            pm: resolve_public_id(&ballot.pm),
                            pm_score: ballot.pm_score,
                            dpm: resolve_public_id(&ballot.dpm),
                            dpm_score: ballot.dpm_score,
                            lo: resolve_public_id(&ballot.lo),
                            lo_score: ballot.lo_score,
                            dlo: resolve_public_id(&ballot.dlo),
                            dlo_score: ballot.dlo_score,
                            mg: resolve_public_id(&ballot.mg),
                            mg_score: ballot.mg_score,
                            gw: resolve_public_id(&ballot.gw),
                            gw_score: ballot.gw_score,
                            mo: resolve_public_id(&ballot.mo),
                            mo_score: ballot.mo_score,
                            ow: resolve_public_id(&ballot.ow),
                            ow_score: ballot.ow_score,
                        };
                        self.client
                            .post(format!("/ballots/{}/submit", key.link))
                            .header(ContentType::Form)
                            .body(serde_urlencoded::to_string(&ballot).unwrap())
                            .dispatch();
                    }
                }
            }
            Action::AddMember(spar_series_member) => {
                let series_idx = (spar_series_member.spar_series_id as usize)
                    .clamp(0, self.spar_series_members.len().saturating_sub(1));
                if let Some(spar_series) = self.spar_series.get(series_idx) {
                    self.client
                        .post(format!(
                            "/spar_series/{}/add_member",
                            spar_series.public_id
                        ))
                        .header(ContentType::Form)
                        .body(
                            &serde_urlencoded::to_string(&AddMemberForm {
                                name: spar_series_member.name.clone(),
                                email: spar_series_member.email.clone(),
                            })
                            .unwrap(),
                        )
                        .dispatch();
                }
            }
            Action::SetSparIsOpen { spar, state } => {
                let spar_idx =
                    (*spar).clamp(0, self.spars.len().saturating_sub(1));
                if let Some(spar) = self.spars.get(spar_idx) {
                    self.client
                        .post(format!(
                            "/spars/{}/set_is_open?state={}",
                            spar.public_id, state
                        ))
                        .dispatch();
                }
            }
        }
    }
}

/// A single action to be performed against the model.
#[derive(DefaultMutator, Arbitrary, Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Create a new user using the `/admin/setup` route.
    Setup(User),
    /// Log in as the nth user in the database. If the user does not exist,
    /// then we do not log in.
    Login(
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        usize,
    ),
    /// Create a new group. This only works if the user is logged in (and has
    /// the correct permissions). If the user is not logged in, then this should
    /// do nothing.
    CreateGroup(Group),
    /// Create a series which groups a number of related spars to a single
    /// object.
    CreateSparSeries(SparSeries),
    AddMember(SparSeriesMember),
    /// Creates a single spar.
    CreateSpar(Spar),
    /// Release draw for a given spar.
    // todo: tests for draw editing
    ReleaseDraw(
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        usize,
    ),
    /// Sign up for the nth spar. If no user is logged in, then this will do
    /// nothing.
    Signup {
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        member_idx: usize,
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        spar_idx: usize,
        as_judge: bool,
        as_speaker: bool,
    },
    /// Generate a draw for the nth spar. Currently, we use the same solver for
    /// both the server and the client here. We do assert that some necessary
    /// properties hold.
    #[field_mutator(
        UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
    )]
    GenerateDraw(
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        usize,
    ),
    /// Submit a ballot in the nth room. Requires that the logged in user is
    /// allocated as a judge for that room.
    SubmitBallot(FuzzerBpBallotForm, usize, usize),
    SetSparIsOpen {
        // todo: weightedusizemutator which
        #[field_mutator(
            UsizeWithinRangeMutator = { usize_within_range_mutator(0..10) }
        )]
        spar: usize,
        state: bool,
    },
}

#[derive(Debug, DefaultMutator, Clone, Serialize, Deserialize, Arbitrary)]
pub struct FuzzerBpBallotForm {
    pub pm: usize,
    pub pm_score: i64,
    pub dpm: usize,
    pub dpm_score: i64,
    pub lo: usize,
    pub lo_score: i64,
    pub dlo: usize,
    pub dlo_score: i64,
    pub mg: usize,
    pub mg_score: i64,
    pub gw: usize,
    pub gw_score: i64,
    pub mo: usize,
    pub mo_score: i64,
    pub ow: usize,
    pub ow_score: i64,
    pub force: bool,
}
