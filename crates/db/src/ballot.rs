use std::collections::HashMap;

use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{adjudicator_ballot_entries, adjudicator_ballots, spar_teams},
    spar::SparRoomTeam,
};

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct AdjudicatorBallotLink {
    pub id: i64,
    pub public_id: String,
    pub link: String,
    pub room_id: i64,
    pub member_id: i64,
    pub created_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct AdjudicatorBallot {
    pub id: i64,
    pub public_id: String,
    pub adjudicator_id: i64,
    pub room_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(
    Queryable,
    Serialize,
    Debug,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Arbitrary,
    Deserialize,
)]
pub struct AdjudicatorBallotEntry {
    pub id: i64,
    pub public_id: String,
    pub ballot_id: i64,
    pub speaker_id: i64,
    pub team_id: i64,
    pub speak: i64,
    pub position: i64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// Note: when comparing ballots you most likely want to compare the
/// [`Scoresheet`]s rather than this struct.
pub struct BallotRepr {
    pub inner: AdjudicatorBallot,
    pub scoresheet: Scoresheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BpTeam {
    Og,
    Oo,
    Cg,
    Co,
}

impl BallotRepr {
    /// Retrieves a useful representation of a ballot given the id of a row in
    /// the adjudicator_ballots table.
    pub fn of_id(
        id: i64,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Result<Self, diesel::result::Error> {
        let ballot = adjudicator_ballots::table
            .filter(adjudicator_ballots::id.eq(id))
            .first::<AdjudicatorBallot>(conn)?;
        let teams = spar_teams::table
            .filter(spar_teams::room_id.eq(ballot.room_id))
            .load::<SparRoomTeam>(conn)?
            .into_iter()
            .map(|team| (team.id, team))
            .collect::<HashMap<i64, _>>();

        let entries = adjudicator_ballot_entries::table
            .filter(adjudicator_ballot_entries::ballot_id.eq(ballot.id))
            .load::<AdjudicatorBallotEntry>(conn)?;

        let mut positions = HashMap::new();
        for entry in entries {
            let scoresheet = SpeakerScoresheet {
                speaker_id: entry.speaker_id,
                score: entry.speak,
            };
            let team = &teams[&entry.team_id];
            positions
                .entry(team.position)
                .and_modify(|speakers: &mut Vec<SpeakerScoresheet>| {
                    assert!(speakers.len() <= 2);
                    speakers.push(scoresheet.clone())
                })
                .or_insert(vec![scoresheet]);
        }
        assert_eq!(positions.len(), 4);

        let scoresheet = Scoresheet {
            teams: {
                positions
                    .into_iter()
                    .sorted_by_key(|(i, _)| *i)
                    .map(|(_, t)| TeamScoresheet { speakers: t })
                    .collect()
            },
        };

        Ok(BallotRepr {
            inner: ballot,
            scoresheet,
        })
    }

    /// Returns the ranking of these teams, with the best team first and the
    /// worst team last.
    ///
    /// Panics if the debate is not currently stored in the database in BP
    /// format.
    pub fn bp_ranking(&self) -> Vec<BpTeam> {
        // todo: maybe return an option instead of asserting here?
        assert_eq!(self.scoresheet.teams.len(), 4);

        self.scoresheet
            .teams
            .iter()
            .enumerate()
            .sorted_by_key(|(_, team)| {
                (-1) * team.speakers.iter().map(|s| s.score).sum::<i64>()
            })
            .map(|(i, _)| {
                let team = match i {
                    0 => BpTeam::Og,
                    1 => BpTeam::Oo,
                    2 => BpTeam::Cg,
                    3 => BpTeam::Co,
                    _ => panic!(),
                };

                team
            })
            .collect()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Scoresheet {
    pub teams: Vec<TeamScoresheet>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TeamScoresheet {
    pub speakers: Vec<SpeakerScoresheet>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SpeakerScoresheet {
    pub speaker_id: i64,
    pub score: i64,
}
