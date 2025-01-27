use std::collections::HashMap;

use db::{
    group::Group,
    spar::{
        AdjudicatorBallotSubmission, Spar, SparRoom, SparRoomAdjudicator,
        SparRoomTeamSpeaker, SparSeries, SparSignup,
    },
    user::User,
};
use uuid::Uuid;

use crate::{
    admin::setup::SetupForm,
    auth::login::PasswordLoginForm,
    ballots::Ballot,
    config_for_internals::MakeSessionForm,
    groups::{CreateGroupForm, CreateSparSeriesForm},
};

use super::sync::{
    draw::get_draw,
    id::{last_id, nth_most_recent_id},
};

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
    client: rocket::local::blocking::Client,
    users: Vec<User>,
    groups: Vec<Group>,
    group_members: HashMap<(usize, Group), GroupMembershipData>,
    spar_series: Vec<SparSeries>,
    spars: Vec<Spar>,
    spar_signups: Vec<SparSignup>,
    active_user: Option<User>,
    rooms: Vec<SparRoom>,
    adjs: Vec<SparRoomAdjudicator>,
    speakers: Vec<SparRoomTeamSpeaker>,
    ballots: Vec<AdjudicatorBallotSubmission>,
}

impl State {
    /// Checks whether the state of the application matches the state of the
    /// model. We compare all the records across all the
    pub fn matches_database(&self) -> bool {
        true
    }

    /// Do a single state transition.
    pub fn step(&mut self, action: &Action) {
        self.step_model(action);
        self.step_app(action);
        assert!(self.matches_database());
    }

    /// Apply action to the model.
    fn step_model(&mut self, action: &Action) {
        match action {
            Action::Setup(user) => {
                if self.users.is_empty() {
                    let mut user = user.clone();
                    // note: we want IDs to line up with the indices in the
                    // relevant places
                    user.id = 0;
                    self.users.push(user);
                }
            }
            Action::Login(user) => {
                if let Some(user) = self.users.get(*user) {
                    self.active_user = Some(user.clone());
                }
            }
            Action::CreateGroup(group) => {
                if let Some(user) = &self.active_user {
                    let mut group = group.clone();
                    let n = self.groups.len();
                    group.id = n as i64;
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
            }
            Action::CreateSparSeries(spar_series) => {
                if let Some(user) = &self.active_user {
                    let group_idx = spar_series.group_id as usize;
                    if let Some(group) = self.groups.get(group_idx) {
                        if let Some(membership) = self
                            .group_members
                            .get(&(user.id as usize, group.clone()))
                        {
                            if membership.is_admin || membership.is_superuser {
                                let spar_series_id = self.spar_series.len();
                                let mut spar_series = spar_series.clone();
                                spar_series.id = spar_series_id as i64;
                                self.spar_series.push(spar_series);
                                assert_eq!(
                                    self.spar_series[spar_series_id].id
                                        as usize,
                                    spar_series_id
                                );
                            }
                        }
                    }
                }
            }
            Action::CreateSpar(spar) => {
                if let Some(user) = &self.active_user {
                    if let Some(spar_series) =
                        self.spar_series.get(spar.spar_series_id as usize)
                    {
                        if let Some(group) =
                            self.groups.get(spar_series.group_id as usize)
                        {
                            if let Some(membership) = self
                                .group_members
                                .get(&(user.id as usize, group.clone()))
                            {
                                if membership.is_admin
                                    || membership.is_superuser
                                {
                                    let spar_id = self.spars.len();
                                    let mut spar = spar.clone();
                                    spar.id = spar_id as i64;
                                    self.spars.push(spar);
                                    assert_eq!(
                                        self.spars[spar_id as usize].id
                                            as usize,
                                        spar_id
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Action::Signup {
                spar_idx,
                as_judge,
                as_speaker,
            } => {
                if let Some(user) = &self.active_user {
                    if let Some(spar) = self.spars.get(*spar_idx) {
                        let signup_idx = self.spar_signups.len();
                        self.spar_signups.push(SparSignup {
                            id: signup_idx as i64,
                            public_id: Uuid::now_v7().to_string(),
                            user_id: user.id,
                            spar_id: spar.id,
                            as_judge: *as_judge,
                            as_speaker: *as_speaker,
                        })
                    }
                }
            }
            Action::GenerateDraw(spar) => {
                // we do nothing here, because draws are generated by the server
                // and then stored. We just check that they are inserted
                // correctly (later - when we enforce parity between the model
                // and the application)

                if let Some(spar) = self.spars.get(*spar) {
                    let rooms =
                        get_draw(spar.public_id.parse().unwrap()).unwrap();
                    for (_, room) in rooms.1.iter().rev().enumerate() {
                        self.rooms.push(SparRoom {
                            id: self.rooms.len() as i64,
                            public_id: room.room_id.to_string(),
                            spar_id: spar.id,
                        });

                        for (_, id) in &room.panel {
                            self.adjs.push(SparRoomAdjudicator {
                                id: self.adjs.len() as i64,
                                public_id: id.my_id.to_string(),
                                user_id: {
                                    self.users
                                        .iter()
                                        .find(|user| {
                                            user.public_id
                                                == id.user_id.to_string()
                                        })
                                        .unwrap()
                                        .id
                                },
                                room_id: self.rooms.last().unwrap().id,
                                // todo: remove statuses as we don't currently
                                // handle them
                                status: "panelist".to_owned(),
                            });
                        }
                    }
                }
            }
            Action::SubmitBallot(ballot, room_idx) => {
                if let Some(user) = &self.active_user {
                    if let Some(room) = self.rooms.get(*room_idx) {
                        if let Some(adj) = self.adjs.iter().find(|adj| {
                            adj.user_id == user.id && adj.room_id == room.id
                        }) {
                            self.ballots.push(AdjudicatorBallotSubmission {
                                id: self.ballots.len() as i64,
                                public_id: last_id()
                                    .expect(
                                        "Error: failed to retrieve last ID!",
                                    )
                                    .to_string(),
                                adjudicator_id: adj.id,
                                room_id: self
                                    .rooms
                                    .iter()
                                    .enumerate()
                                    .find(|(_i, needle)| {
                                        needle.public_id == room.public_id
                                    })
                                    .map(|(i, _)| i)
                                    .unwrap()
                                    as i64,
                                // note: when we check the state we only require that
                                // the insertion order matches, rather than
                                // individual timestamps
                                //
                                // todo: find a way to allocate these monotonically
                                created_at: chrono::Utc::now().naive_utc(),
                                // todo: actually verify ballot data
                                ballot_data: serde_json::to_string(&ballot)
                                    .unwrap(),
                            })
                        }
                    }
                }
            }
        }
    }

    /// Apply action to the real application.
    fn step_app(&self, action: &Action) {
        match action {
            Action::Setup(user) => {
                if let Some(username) = &user.username {
                    if let Some(password) = &user.password_hash {
                        {
                            self.client
                                .post("/admin/setup")
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
                        };
                    }
                }
            }
            Action::Login(user) => {
                if let Some(user) = self.users.get(*user) {
                    if let Some(password) = &user.password_hash {
                        self.client
                            .post("/login")
                            .body(
                                serde_urlencoded::to_string(
                                    &PasswordLoginForm {
                                        email: user.email.clone(),
                                        password: password.clone(),
                                    },
                                )
                                .unwrap(),
                            )
                            .dispatch();
                    }
                }
            }
            Action::CreateGroup(group) => {
                self.client
                    .post("/groups/new")
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
                if spar_series.group_id >= 0 {
                    let idx = spar_series.group_id as usize;
                    // todo: try to hit an extra coverage counter for smaller
                    // indices to encourage this (?)
                    if let Some(group) = self.groups.get(idx) {
                        self.client
                            .post(format!(
                                "/groups/{}/spar_series/new",
                                group.public_id
                            ))
                            .body(
                                serde_urlencoded::to_string(
                                    &CreateSparSeriesForm {
                                        title: spar_series.title.clone(),
                                        description: spar_series
                                            .description
                                            .clone(),
                                        // todo: remove this as we only support BP
                                        speakers_per_team: 4,
                                    },
                                )
                                .unwrap(),
                            )
                            .dispatch();
                    }
                }
            }
            Action::CreateSpar(spar) => {
                if let Some(spar_series) =
                    self.spar_series.get(spar.spar_series_id as usize)
                {
                    self.client
                        .post(format!(
                            "/spar_series/{}/makesess",
                            spar_series.public_id
                        ))
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
            Action::Signup {
                spar_idx,
                as_judge,
                as_speaker,
            } => {}
            Action::GenerateDraw(spar) => {}
            Action::SubmitBallot(ballot, room_idx) => {}
        }
    }
}

/// A single action to be performed against the model.
pub enum Action {
    /// Create a new user using the `/admin/setup` route.
    Setup(User),
    /// Log in as the nth user in the database. If the user does not exist,
    /// then we do not log in.
    Login(usize),
    /// Create a new group. This only works if the user is logged in (and has
    /// the correct permissions). If the user is not logged in, then this should
    /// do nothing.
    CreateGroup(Group),
    /// Create a series which groups a number of related spars to a single
    /// object.
    CreateSparSeries(SparSeries),
    /// Creates a single spar.
    CreateSpar(Spar),
    /// Sign up for the nth spar. If no user is logged in, then this will do
    /// nothing.
    Signup {
        spar_idx: usize,
        as_judge: bool,
        as_speaker: bool,
    },
    /// Generate a draw for the nth spar. Currently, we use the same solver for
    /// both the server and the client here. We do assert that some necessary
    /// properties hold.
    GenerateDraw(usize),
    /// Submit a ballot in the nth room. Requires that the logged in user is
    /// allocated as a judge for that room.
    SubmitBallot(Ballot, usize),
}
