use db::{user::User, DbConn};
use diesel::Connection;
use maud::{html, Markup};

use crate::{
    html::{error_403, page_of_body, page_title},
    permissions::has_permission,
};

pub mod config;
pub mod invite;
pub mod setup;

#[get("/admin")]
pub async fn admin_overview(user: User, db: DbConn) -> Result<Markup, Markup> {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &crate::permissions::Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Err(error_403(
                    Some("Error: you are not authorized to view this page"),
                    Some(user),
                )));
            }

            let page_title = page_title("Admin page");
            let page_body = html! {
                ul class="list-group" {
                    li class="list-group-item" {
                        a href="/admin/invite" {
                            "Send an account invite"
                        }
                    }
                }
            };

            let markup = page_of_body(
                html! {
                    (page_title)
                    (page_body)
                },
                Some(user),
            );

            Ok(Ok(markup))
        })
        .unwrap()
    })
    .await
}
