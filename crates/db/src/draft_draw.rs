use diesel::prelude::Queryable;
use fuzzcheck::DefaultMutator;
use fuzzcheck_util::chrono_mutators::{
    naive_date_time_mutator, NaiveDateTimeMutator,
};
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Queryable,
    Serialize,
    Deserialize,
    Clone,
    Hash,
    PartialEq,
    Eq,
    DefaultMutator,
)]
pub struct DraftDraw {
    pub id: i64,
    pub public_id: String,
    pub data: Option<String>,
    pub spar_id: i64,
    pub version: i64,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: chrono::NaiveDateTime,
}
