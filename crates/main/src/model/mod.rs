// NOTE: VERY WIP

use std::collections::HashMap;

use db::{
    inst::Group,
    spar::{Spar, SparSeries, SparSignup},
    user::User,
};
use uuid::Uuid;

use crate::ballots::Ballot;

pub struct GroupMembershipData {
    is_admin: bool,
    is_superuser: bool,
}

/// The state of the model.
pub struct State {
    _client: rocket::local::blocking::Client,
    users: Vec<User>,
    groups: Vec<Group>,
    group_members: HashMap<(usize, Group), GroupMembershipData>,
    spar_series: Vec<SparSeries>,
    spars: Vec<Spar>,
    spar_signups: Vec<SparSignup>,
    active_user: Option<User>,
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
                            session_id: spar.id,
                            as_judge: *as_judge,
                            as_speaker: *as_speaker,
                        })
                    }
                }
            }
            Action::GenerateDraw(_) => todo!(),
            Action::SubmitBallot(_, _) => todo!(),
        }
    }

    /// Apply action to the real application.
    fn step_app(&self, action: &Action) {
        match action {
            Action::Setup(_user) => todo!(),
            Action::Login(_) => todo!(),
            Action::CreateGroup(_group) => todo!(),
            Action::CreateSparSeries(_spar_series) => todo!(),
            Action::CreateSpar(_spar) => todo!(),
            Action::Signup {
                spar_idx: _,
                as_judge: _,
                as_speaker: _,
            } => todo!(),
            Action::GenerateDraw(_) => todo!(),
            Action::SubmitBallot(_ballot, _) => todo!(),
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
