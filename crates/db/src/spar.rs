use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use fuzzcheck::mutators::option::OptionMutator;
use fuzzcheck::DefaultMutator;
use fuzzcheck_util::chrono_mutators::{
    naive_date_time_mutator, NaiveDateTimeMutator,
};
use fuzzcheck_util::useful_string_mutator::{
    useful_string_mutator, UsefulStringMutator,
};
use serde::{Deserialize, Serialize};

#[derive(
    Queryable,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Hash,
    PartialEq,
    Eq,
    Arbitrary,
    DefaultMutator,
)]
pub struct SparSeries {
    pub id: i64,
    pub public_id: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub title: String,
    #[field_mutator(OptionMutator<String, UsefulStringMutator> = { OptionMutator::new(useful_string_mutator()) })]
    pub description: Option<String>,
    pub speakers_per_team: i64,
    pub group_id: i64,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
    pub allow_join_requests: bool,
    pub auto_approve_join_requests: bool,
}

#[derive(
    Queryable,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Hash,
    Eq,
    PartialEq,
    Arbitrary,
    DefaultMutator,
)]
pub struct Spar {
    pub id: i64,
    pub public_id: String,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub start_time: NaiveDateTime,
    pub is_open: bool,
    pub release_draw: bool,
    pub spar_series_id: i64,
    pub is_complete: bool,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
}

allow_columns_to_appear_in_same_group_by_clause!(
    adjudicator_ballots::id,
    spar_rooms::id
);

impl Spar {
    /// Returns the canonical ballot for each room in this spar.
    ///
    /// Currently we resolve conflicting ballots by assuming that the most
    /// recently submitted ballot is correct.
    pub fn canonical_ballots(
        &self,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<BallotRepr>, diesel::result::Error> {
        let (b1, b2) = diesel::alias!(
            adjudicator_ballots as b1,
            adjudicator_ballots as b2
        );

        let ballots = b1
            .inner_join(spar_rooms::table)
            .filter(spar_rooms::spar_id.eq(self.id))
            .group_by(spar_rooms::id)
            .filter(
                b1.field(adjudicator_ballots::created_at).nullable().eq(b2
                    .filter(
                        b2.field(adjudicator_ballots::room_id)
                            .eq(b1.field(adjudicator_ballots::room_id)),
                    )
                    .order_by(b2.field(adjudicator_ballots::created_at).desc())
                    .limit(1)
                    .select(b2.field(adjudicator_ballots::created_at))
                    .single_value()),
            )
            .select((
                spar_rooms::id,
                diesel::dsl::max(b1.field(adjudicator_ballots::id)),
            ))
            .load::<(i64, Option<i64>)>(conn)?;

        let mut ret = Vec::with_capacity(ballots.len());

        for (_, ballot_id) in ballots {
            let ballot_id = ballot_id.expect(
                "error: the query should always
                 return at least one ballot for each room (no records should be
                 returned if there are no ballots)",
            );

            ret.push(BallotRepr::of_id(ballot_id, conn)?);
        }

        Ok(ret)
    }

    /// Loads all the rooms that are part of this spar.
    pub fn rooms(
        &self,
        conn: &mut SqliteConnection,
    ) -> Result<Vec<SparRoomRepr>, diesel::result::Error> {
        let rooms = spar_rooms::table
            .filter(spar_rooms::spar_id.eq(self.id))
            .load::<SparRoom>(conn)?;

        let mut ret = Vec::with_capacity(rooms.len());

        for room in rooms {
            ret.push(SparRoomRepr::of_id(room.id, conn)?);
        }

        Ok(ret)
    }
}

#[derive(Debug, Queryable, Serialize, Clone, Arbitrary, DefaultMutator)]
pub struct SparSignup {
    pub id: i64,
    pub public_id: String,
    pub member_id: i64,
    pub spar_id: i64,
    pub as_judge: bool,
    pub as_speaker: bool,
}

#[derive(Debug, Serialize)]
pub struct SparSignupSerializer {
    pub id: i64,
    pub public_id: String,
    pub member: SparSeriesMember,
    pub session_id: i64,
    pub as_judge: bool,
    pub as_speaker: bool,
}

impl SparSignupSerializer {
    pub fn from_db_ty(
        t: SparSignup,
        conn: &mut SqliteConnection,
    ) -> SparSignupSerializer {
        Self {
            id: t.id,
            public_id: t.public_id,
            member: crate::schema::spar_series_members::table
                .filter(crate::schema::spar_series_members::id.eq(t.member_id))
                .get_result::<SparSeriesMember>(conn)
                .unwrap(),
            session_id: t.spar_id,
            as_judge: t.as_judge,
            as_speaker: t.as_speaker,
        }
    }
}

#[derive(Queryable, Arbitrary, Debug, Serialize, Deserialize, Clone)]
pub struct SparRoomTeam {
    pub id: i64,
    pub public_id: String,
    pub room_id: i64,
    pub position: i64,
}

use crate::ballot::BallotRepr;
use crate::room::SparRoomRepr;
use crate::schema::{adjudicator_ballots, spar_rooms, spar_series_members};

#[derive(
    Queryable,
    QueryableByName,
    Arbitrary,
    Debug,
    Serialize,
    Deserialize,
    Identifiable,
    DefaultMutator,
    Clone,
)]
/// A member of a spar. These are not full user accounts, and instead function
/// similar to how Tabbycat implements private URLs. Team members are just
/// recorded in the system. Judges are emailed a link to fill out their ballots
/// when a spar begins.
pub struct SparSeriesMember {
    pub id: i64,
    pub public_id: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub name: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub email: String,
    pub spar_series_id: i64,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct SparRoomTeamSpeaker {
    pub id: i64,
    pub public_id: String,
    pub member_id: i64,
    pub team_id: i64,
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
    DefaultMutator,
)]
pub struct SparRoomAdjudicator {
    pub id: i64,
    pub public_id: String,
    pub member_id: i64,
    pub room_id: i64,
    // one of "chair", "panelist", "trainee"
    pub status: String,
}

pub use crate::room::SparRoom;
