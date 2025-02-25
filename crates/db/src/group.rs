use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::{
    dsl::auto_type, prelude::*, sql_types::Bool, sqlite::Sqlite,
    BoxableExpression,
};

use fuzzcheck::DefaultMutator;
use fuzzcheck_util::{
    chrono_mutators::{naive_date_time_mutator, NaiveDateTimeMutator},
    useful_string_mutator::{useful_string_mutator, UsefulStringMutator},
};
use serde::{Deserialize, Serialize};

use crate::schema::{self, group_members, groups};

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
pub struct Group {
    pub id: i64,
    pub public_id: String,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub name: String,
    pub website: Option<String>,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
}

impl Group {
    pub fn with_name<'a>(
        name: &'a str,
    ) -> Box<
        dyn BoxableExpression<schema::groups::table, Sqlite, SqlType = Bool>
            + 'a,
    > {
        Box::new(groups::name.eq(name))
    }

    pub fn validate_name(name: &str) -> bool {
        name.chars().count() > 3
            && name
                .chars()
                .all(|c| c.is_ascii() && (c.is_alphanumeric() || c.is_ascii()))
    }
}

#[cfg(test)]
#[test]
fn test_group_validate() {
    assert!(Group::validate_name("usefulAsciiString"))
}

#[derive(Debug, Queryable, Serialize, Arbitrary)]
pub struct GroupMember {
    pub id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub has_signing_power_bool: bool,
    pub is_admin: bool,
}

impl GroupMember {
    #[auto_type(no_type_alias)]
    pub fn is_admin() -> _ {
        group_members::is_admin
            .eq(true)
            .or(group_members::has_signing_power.eq(true))
    }

    #[auto_type(no_type_alias)]
    pub fn has_signing_power() -> _ {
        group_members::has_signing_power.eq(true)
    }
}
