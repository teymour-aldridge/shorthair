use db::{
    group::Group,
    schema::{group_members, groups},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use diesel::{Connection, QueryDsl};
use maud::Markup;
use tracing::Instrument;

use crate::{html::page_of_body, request_ids::TracingSpan};

#[get("/user")]
pub async fn account_page(user: User, db: DbConn, span: TracingSpan) -> Markup {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let groups_user_belongs_to = groups::table
                .inner_join(group_members::table)
                .filter(group_members::user_id.eq(&user.id))
                .select(groups::all_columns)
                .load::<Group>(conn)?;

            let markup = maud::html!(
                h1 {"Hello " (user.username.clone().unwrap_or("Unnamed user".to_string()))}

                h3 {"Quick actions"}

                ul {
                    li {
                        a href="/user/auth" {"Change password"}
                    }
                    li {
                        // todo: check if user has permission to create
                        // resources
                        a href="/groups/new" {"Create a new group"}
                    }
                }

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
    .instrument(span.0)
    .await
}
