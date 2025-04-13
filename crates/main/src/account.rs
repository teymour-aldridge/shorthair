use db::{
    group::Group,
    schema::{group_members, groups},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use diesel::{Connection, QueryDsl};
use maud::Markup;

use crate::html::page_of_body;

#[get("/user")]
pub async fn account_page(user: User, db: DbConn) -> Markup {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let groups_user_belongs_to = groups::table
                .inner_join(group_members::table)
                .filter(group_members::user_id.eq(&user.id))
                .select(groups::all_columns)
                .load::<Group>(conn)?;

            let markup = maud::html!(
                h1 {"Hello " (user.username.clone().unwrap_or("Unnamed user".to_string()))}
                h3 {"My groups"}
                table class="table" {
                    thead {
                        tr {
                            th scope="col" {"Group Name"}
                            th scope="col" {"View group"}
                        }
                    }
                    tbody {
                        @for group in groups_user_belongs_to {
                            tr {
                                th scope="row" {(group.name)}
                                td {a href=(format!("/groups/{}", group.public_id)) {"View group"}}
                            }
                        }
                    }
                }
            );

            Ok(page_of_body(markup, Some(user)))
        })
        .unwrap()
    })
    .await
}
