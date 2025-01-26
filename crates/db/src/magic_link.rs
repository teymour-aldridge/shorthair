use chrono::{NaiveDateTime, Utc};
use diesel::{prelude::*, sql_types::Bool, sqlite::Sqlite};

use crate::schema::magic_links;

#[derive(Debug, Queryable)]
pub struct MagicLink {
    pub id: i64,
    pub code: String,
    pub user_id: i64,
    pub created_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
    pub already_used: bool,
}

impl MagicLink {
    pub fn valid_with_code<'a>(
        code: &'a str,
    ) -> Box<
        dyn BoxableExpression<magic_links::table, Sqlite, SqlType = Bool> + 'a,
    > {
        Box::new(
            magic_links::code
                .eq(code)
                .and(magic_links::expires_at.gt(Utc::now().naive_utc()))
                .and(magic_links::already_used.eq(false)),
        )
    }
}
