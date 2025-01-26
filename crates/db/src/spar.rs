use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;

use crate::user::User;

#[derive(Queryable, Serialize, Debug, Clone, Hash, PartialEq, Eq)]
pub struct SparSeries {
    pub id: i64,
    pub public_id: String,
    pub title: String,
    pub description: Option<String>,
    pub speakers_per_team: i64,
    pub group_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Queryable, Serialize, Clone, Debug, Hash, Eq, PartialEq)]
pub struct Spar {
    pub id: i64,
    pub public_id: String,
    pub start_time: NaiveDateTime,
    pub is_open: bool,
    pub release_draw: bool,
    pub spar_series_id: i64,
    pub created_at: NaiveDateTime,
}

#[derive(Queryable, Serialize, Clone)]
pub struct SparSignup {
    pub id: i64,
    pub public_id: String,
    /// The id of the user, as in the database (i.e. NOT in Tabbycat).
    pub user_id: i64,
    pub session_id: i64,
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
            session_id: t.session_id,
            as_judge: t.as_judge,
            as_speaker: t.as_speaker,
        }
    }
}

#[derive(Queryable)]
pub struct SparRoomAdjudicator {
    pub id: i64,
    pub public_id: String,
    pub user_id: i64,
    pub room_id: i64,
    // one of "chair", "panelist", "trainee"
    pub status: String,
}

#[derive(Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct AdjudicatorBallotSubmission {
    pub id: i64,
    pub public_id: String,
    pub adjudicator_id: i64,
    pub room_id: i64,
    pub created_at: String,
    pub ballot_data: String,
}

#[derive(Queryable, Serialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct SparRoom {
    pub id: i64,
    pub public_id: String,
    pub spar_id: i64,
}
