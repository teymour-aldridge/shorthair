use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use fuzzcheck::DefaultMutator;
use fuzzcheck_util::{
    chrono_mutators::{naive_date_time_mutator, NaiveDateTimeMutator},
    useful_string_mutator::{useful_string_mutator, UsefulStringMutator},
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
    Arbitrary,
    DefaultMutator,
)]

pub struct AccountInvite {
    pub id: i64,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub public_id: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub code: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub email: String,
    pub sent_by: i64,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
    pub may_create_resources: bool,
}
