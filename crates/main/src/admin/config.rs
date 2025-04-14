use db::config::ConfigItem;
use db::{schema::config, user::User, DbConn};
use diesel::prelude::*;
use diesel::Connection;
use maud::Markup;
use rocket::form::{Form, FromForm};
use rocket::response::Redirect;
use uuid::Uuid;

use crate::html::{page_of_body, page_title};
use crate::{
    html::error_403,
    permissions::{has_permission, Permission},
};

#[get("/admin/config")]
pub async fn config_page(user: User, db: DbConn) -> Option<Markup> {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Some(error_403(
                    Some("Error: you are not authorized to view this page!"),
                    Some(user),
                )));
            }

            let config_items = config::table
                .order_by(config::key.asc())
                .load::<ConfigItem>(conn)
                .unwrap();

            let markup = maud::html! {
                table class="table" {
                    thead {
                        tr {
                            th scope="col" {
                                "Key"
                            }
                            th scope="col" {
                                "Value"
                            }
                            th scope="col" {
                                "Edit"
                            }
                        }
                    }
                    tbody {
                        @for config in config_items.iter() {
                            tr {
                                td {
                                    (config.key)
                                }
                                td {
                                    (config.value)
                                }
                                td {
                                    a href=(format!("/admin/config/{}/edit", config.public_id)) {
                                        "Edit"
                                    }
                                }
                            }
                        }
                    }
                }
            };

            let create_form_markup = maud::html! {
                form action="/admin/config/upsert" method="post" {
                    div class="mb-3" {
                        label for="key" class="form-label" { "Key" }
                        input type="text" class="form-control" id="key" name="key" placeholder="key";
                    }
                    div class="mb-3" {
                        label for="value" class="form-label" { "Value" }
                        input type="text" class="form-control" id="value" name="value" placeholder="value";
                    }
                    button type="submit" class="btn btn-primary" { "Add Item" }
                }
            };

            let title = page_title("Site configuration");

            Ok(Some(page_of_body(
                maud::html! {
                    (title)
                    h2 {"Current config items"}
                    (markup)
                    h2 {"Add new config item"}
                    (create_form_markup)
                },
                Some(user),
            )))
        })
        .unwrap()
    })
    .await
}

#[get("/admin/config/<config_id>/edit")]
pub async fn edit_existing_config_item_page(
    db: DbConn,
    user: User,
    config_id: &str,
) -> Option<Markup> {
    let config_id = config_id.to_string();

    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Some(error_403(
                    Some("Error: you are not authorized to view this page!"),
                    Some(user),
                )));
            }

            let config_object =
                config::table.filter(config::public_id.eq(config_id));

            let config_item = config_object.first::<ConfigItem>(conn)?;

            let title = page_title("Edit config item");
            let markup = maud::html! {
                form action=("/admin/config/upsert") method="post" {
                    div class="mb-3" {
                        label for="key" class="form-label" { "Key" }
                        input type="text" class="form-control" id="key" name="key" value=(config_item.key) readonly="readonly";
                    }
                    div class="mb-3" {
                        label for="value" class="form-label" { "Value" }
                        input type="text" class="form-control" id="value" name="value" value=(config_item.value);
                    }
                    button type="submit" class="btn btn-primary" { "Save changes" }
                }
            };

            Ok(Some(page_of_body(
                maud::html! {
                    (title)
                    (markup)
                },
                Some(user),
            )))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm)]
pub struct UpsertConfigForm {
    key: String,
    value: String,
}

#[post("/admin/config/upsert", data = "<form>")]
pub async fn do_upsert_config(
    db: DbConn,
    user: User,
    form: Form<UpsertConfigForm>,
) -> Option<Result<Redirect, Markup>> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Some(Err(error_403(
                    Some(
                        "Error: you are not authorized to perform this action!",
                    ),
                    Some(user),
                ))));
            }

            let n_updated = diesel::insert_into(config::table)
                .values((
                    config::public_id.eq(Uuid::new_v4().to_string()),
                    config::key.eq(&form.key),
                    config::value.eq(&form.value),
                ))
                .on_conflict(config::key)
                .do_update()
                .set(config::value.eq(&form.value))
                .execute(conn)
                .unwrap();
            assert_eq!(n_updated, 1);

            Ok(Some(Ok(Redirect::to("/admin/config"))))
        })
        .unwrap()
    })
    .await
}
