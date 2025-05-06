//! Represents a complete room. As rooms are represented in relational form in
//! the database, they can be a little bit tricky to work with.

use std::collections::HashMap;

use arbitrary::Arbitrary;
use diesel::connection::LoadConnection;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use fuzzcheck::DefaultMutator;
use serde::Serialize;

use crate::ballot::BallotRepr;
use crate::schema::adjudicator_ballots;
use crate::schema::spar_adjudicators;
use crate::schema::spar_series_members;
use crate::schema::spar_speakers;
use crate::schema::spar_teams;
use crate::{
    schema::spar_rooms,
    spar::{
        SparRoomAdjudicator, SparRoomTeam, SparRoomTeamSpeaker,
        SparSeriesMember,
    },
};

#[derive(
    Queryable,
    Serialize,
    Debug,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Arbitrary,
    DefaultMutator,
)]
pub struct SparRoom {
    pub id: i64,
    pub public_id: String,
    pub spar_id: i64,
}

impl SparRoom {
    #[tracing::instrument(name = "SparRoom::repr", skip(conn))]
    pub fn repr(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Result<SparRoomRepr, diesel::result::Error> {
        SparRoomRepr::of_id(self.id, conn)
    }

    #[tracing::instrument(name = "SparRoom::canonical_ballot", skip(conn))]
    pub fn canonical_ballot(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Result<Option<BallotRepr>, diesel::result::Error> {
        let id = adjudicator_ballots::table
            .filter(adjudicator_ballots::room_id.eq(self.id))
            .order_by(adjudicator_ballots::created_at)
            .select(adjudicator_ballots::id)
            .first::<i64>(conn)
            .optional()?;

        match id {
            Some(id) => Ok(Some(BallotRepr::of_id(id, conn)?)),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SparRoomRepr {
    /// The teams will be stored here in the order they speak.
    ///
    /// For BP teams[0] = og, teams[1] = oo, teams[2] = cg, teams[3] = co
    pub inner: SparRoom,
    /// List of all teams assigned to this room.
    pub teams: Vec<TeamRepr>,
    /// Maps speakers to relevant records.
    pub speakers: HashMap<i64, SparRoomTeamSpeaker>,
    /// List of all judges assigned to this room.
    pub judges: Vec<SparRoomAdjudicator>,
    /// Maps member IDs to records.
    pub members: HashMap<i64, SparSeriesMember>,
}

impl SparRoomRepr {
    #[tracing::instrument(name = "SparRoomRepr::of_id", skip(conn))]
    pub fn of_id(
        room_id: i64,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Result<Self, diesel::result::Error> {
        let room = spar_rooms::table
            .filter(spar_rooms::id.eq(room_id))
            .first::<SparRoom>(conn)
            .unwrap();

        let teams = spar_teams::table
            .filter(spar_teams::room_id.eq(room.id))
            .order_by(spar_teams::position.asc())
            .load::<SparRoomTeam>(conn)
            .unwrap();
        assert_eq!(teams.len(), 4);

        let speakers = spar_speakers::table
            .inner_join(spar_teams::table)
            .filter(spar_teams::room_id.eq(room.id))
            // note: this ordering is an important invariant that other parts of
            // the application rely on
            .order_by(spar_speakers::id.asc())
            .select(spar_speakers::all_columns)
            .load::<SparRoomTeamSpeaker>(conn)
            .unwrap();

        let team_repr = {
            let mut team_repr = Vec::with_capacity(teams.len());
            for team in teams {
                team_repr.push(TeamRepr {
                    inner: team.clone(),
                    speakers: speakers
                        .iter()
                        .filter(|speaker| speaker.team_id == team.id)
                        .map(|speaker| speaker.id)
                        .collect(),
                })
            }
            team_repr
        };

        let judges = spar_adjudicators::table
            .filter(spar_adjudicators::room_id.eq(room.id))
            .load::<SparRoomAdjudicator>(conn)?;

        // members = all judges and teams
        let mut members = spar_series_members::table
            .inner_join(spar_speakers::table.inner_join(spar_teams::table))
            .filter(spar_teams::room_id.eq(room.id))
            .select(spar_series_members::all_columns)
            .load(conn)?;
        members.extend(
            spar_series_members::table
                .inner_join(spar_adjudicators::table)
                .filter(spar_adjudicators::room_id.eq(room.id))
                .select(spar_series_members::all_columns)
                .load(conn)?,
        );

        Ok(SparRoomRepr {
            inner: room,
            teams: team_repr,
            speakers: speakers
                .into_iter()
                .map(|speaker| (speaker.id, speaker))
                .collect(),
            judges,
            members: members
                .into_iter()
                .map(|member: SparSeriesMember| (member.id, member))
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TeamRepr {
    pub inner: SparRoomTeam,
    pub speakers: Vec<i64>,
}
