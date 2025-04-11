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
pub struct EmailRow {
    pub id: i64,
    pub message_id: String,
    pub recipients: String,
    pub contents: Option<String>,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: chrono::NaiveDateTime,
}
