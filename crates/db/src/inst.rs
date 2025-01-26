use chrono::NaiveDateTime;
use diesel::{
    dsl::auto_type, prelude::*, sql_types::Bool, sqlite::Sqlite,
    BoxableExpression,
};

use serde::Serialize;

use crate::schema::{self, group_members, groups};

#[derive(Debug, Queryable, Serialize, Clone, Hash, PartialEq, Eq)]
pub struct Group {
    pub id: i64,
    pub public_id: String,
    pub name: String,
    pub website: Option<String>,
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
}

#[derive(Debug, Queryable, Serialize)]
pub struct GroupMember {
    rowid: i64,
    user_id: i64,
    institution_id: i64,
    has_signing_power_bool: bool,
    is_admin: bool,
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
