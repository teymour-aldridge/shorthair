use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;

use crate::user::User;

#[derive(
    Queryable, Serialize, Debug, Clone, Hash, PartialEq, Eq, Arbitrary,
)]
pub struct SparSeries {
    pub id: i64,
    pub public_id: String,
    pub title: String,
    pub description: Option<String>,
    pub speakers_per_team: i64,
    pub group_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(
    Queryable, Serialize, Clone, Debug, Hash, Eq, PartialEq, Arbitrary,
)]
pub struct Spar {
    pub id: i64,
    pub public_id: String,
    pub start_time: NaiveDateTime,
    pub is_open: bool,
    pub release_draw: bool,
    pub spar_series_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Queryable, Serialize, Clone, Arbitrary)]
pub struct SparSignup {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub spar_id: i64,
    pub as_judge: bool,
    pub as_speaker: bool,
}

#[derive(Debug, Serialize)]
pub struct SparSignupSerializer {
    pub id: i64,
    pub public_id: String,
    pub user: User,
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
            user: crate::schema::users::table
                .filter(crate::schema::users::id.eq(t.user_id))
                .get_result::<User>(conn)
                .unwrap(),
            session_id: t.spar_id,
            as_judge: t.as_judge,
            as_speaker: t.as_speaker,
        }
    }
}

#[derive(Queryable, Arbitrary)]
pub struct SparRoomTeam {
    pub id: i64,
    pub public_id: String,
    pub room_id: i64,
    pub position: i64,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct SparRoomTeamSpeaker {
    id: i64,
    public_id: String,
    user_id: i64,
    team_id: i64,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct SparRoomAdjudicator {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub room_id: i64,
    // one of "chair", "panelist", "trainee"
    pub status: String,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct AdjudicatorBallotSubmission {
    pub id: i64,
    pub public_id: String,
    pub adjudicator_id: i64,
    pub room_id: i64,
    pub created_at: NaiveDateTime,
    pub ballot_data: String,
}

#[derive(
    Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash, Arbitrary,
)]
pub struct SparRoom {
    pub id: i64,
    pub public_id: String,
    pub spar_id: i64,
}
