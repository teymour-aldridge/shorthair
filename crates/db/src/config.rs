use diesel::Queryable;
use fuzzcheck::DefaultMutator;
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
pub struct ConfigItem {
    pub id: i64,
    pub public_id: String,
    pub key: String,
    pub value: String,
}
