use chrono::Utc;
use db::{
    group::Group,
    schema::{group_members, groups, spar_series},
    spar::SparSeries,
    user::User,
    DbConn,
};
use diesel::{
    dsl::{exists, insert_into, select},
    prelude::*,
};
use either::Either;
use maud::{html, Markup};
use rocket::{
    form::{Form, FromForm},
    response::{Flash, Redirect},
};
use serde::Serialize;
use tracing::Instrument;

use crate::{
    model::sync::id::gen_uuid,
    page_of_body,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    resources::GroupRef,
};

fn create_group_form(error: Option<String>) -> Markup {
    html! {
        div class="container mt-5" {
            @if let Some(err) = error {
                div class="alert alert-danger" role="alert" {
                    (err)
                }
            }
            form method="post" {
                div class="form-group" {
                    label for="name" { "Name" }
                    input type="text" class="form-control" id="name" name="name" required {}
                }
                div class="form-group" {
                    label for="website" { "Website (optional)" }
                    input type="url" class="form-control" id="website" name="website" {}
                }
                button type="submit" class="btn btn-primary" { "Submit" }
            }
        }
    }
}

#[get("/groups/new")]
pub async fn create_group_page(user: User) -> Markup {
    page_of_body(create_group_form(None), Some(user))
}

#[derive(FromForm, Serialize)]
pub struct CreateGroupForm {
    pub name: String,
    pub website: Option<String>,
}

#[post("/groups/new", data = "<group>")]
pub async fn do_create_group(
    user: User,
    group: Form<CreateGroupForm>,
    db: DbConn,
    span: TracingSpan,
) -> Either<Markup, Redirect> {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let name_taken = select(exists(groups::table.filter(groups::name.eq(&group.name))))
                .get_result::<bool>(conn)
                .unwrap();

            if name_taken {
                let markup = create_group_form(Some(
                    "A group with that name already exists - please select a different name."
                        .to_string(),
                ));
                return Ok(Either::Left(page_of_body(markup, Some(user))));
            }

            if !Group::validate_name(&group.name) {
                let markup = create_group_form(Some(
                    "Error: that name is not valid (group names may be at most
                     32 characters long)."
                        .to_string(),
                ));
                return Ok(Either::Left(page_of_body(markup, Some(user))));
            }

            let website_taken = if group.website.is_some() {
                select(exists(groups::table.filter(groups::website.eq(&group.website)))).get_result::<bool>(conn).unwrap()
            } else {
                false
            };
            if website_taken {
                let markup = create_group_form(Some(
                    "Error: a different group with that website exists. (Note: a
                        website can only be used by a single group!)"
                        .to_string(),
                ));
                return Ok(Either::Left(page_of_body(markup, Some(user))));

            }

            let group_public_id = gen_uuid().to_string();

            let id = diesel::insert_into(groups::table)
                .values((
                    groups::public_id.eq(&group_public_id),
                    groups::name.eq(&group.name),
                    groups::website.eq(&group.website),
                    groups::created_at.eq(diesel::dsl::now),
                ))
                .returning(groups::id)
                .get_result::<i64>(conn)
                .unwrap();

            let n = diesel::insert_into(group_members::table)
                .values((
                    group_members::group_id.eq(id),
                    group_members::user_id.eq(user.id),
                    group_members::has_signing_power.eq(true),
                    group_members::is_admin.eq(true),
                ))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            Ok(Either::Right(Redirect::temporary(format!(
                "/groups/{}",
                group_public_id
            ))))
        })
        .unwrap()
    })
    .await
}

#[get("/groups/<group_id>")]
pub async fn view_group(
    group_id: String,
    db: DbConn,
    user: User,
    span: TracingSpan,
) -> Option<Markup> {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let group = match groups::table
                .filter(groups::public_id.eq(group_id))
                .get_result::<Group>(conn)
                .optional()
                .unwrap() {
                    Some(t) => t,
                    None => {return Ok(None)}
                };

            let has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef(group.id)),
                conn,
            );

            if !has_permission {
                return Ok(None);
            }


            let spar_series = spar_series::table
                .filter(spar_series::group_id.eq(group.id))
                .load::<SparSeries>(conn).unwrap();

            Ok(Some(page_of_body(html! {
                h1 { "Group: " (group.name) }

                @if !spar_series.is_empty() {
                    h3 { "Spars" }
                    table class="table" {
                        thead {
                            tr {
                                th scope="col" { "Series title" }
                                th { "View series" }
                            }
                        }
                        tbody {
                            @for series in spar_series {
                                tr {
                                    th scope="row" { (series.title) }
                                    td {
                                        a href=(format!("/spar_series/{}", series.public_id)) {
                                            "View series"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div class="mt-4" {
                    a class="btn btn-primary" href={ "/groups/" (group.public_id) "/spar_series/new" } {
                        "New internal spar"
                    }
                }
            }, Some(user))))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

#[get("/groups/<group_id>/spar_series/new")]
pub async fn create_new_spar_series_page(
    group_id: &str,
    user: User,
    db: DbConn,
    span: TracingSpan,
) -> Option<Result<Markup, Flash<Redirect>>> {
    let group_id = group_id.to_string();
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let group = match groups::table
                .filter(groups::public_id.eq(&group_id))
                .get_result::<Group>(conn)
                .optional()
                .unwrap()
            {
                Some(inst) => inst,
                None => return Ok(None),
            };

            let has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef(group.id)),
                conn,
            );

            if !has_permission {
                return Ok(Some(Err(Flash::error(
                    Redirect::to("/"),
                    "Error: you do not have permission to do that!",
                ))));
            }

            Ok(Some(Ok(page_of_body(
                make_new_spar_series_form(),
                Some(user),
            ))))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

fn make_new_spar_series_form() -> Markup {
    html! {
        h1 { "Create spar series" }
        div class="alert alert-info" role="alert" {
            "Note: a spar series connects a number of spars together.
             When generating a draw for a new spar in the series, only
             data from previous spars"
             i {" in the same series "}
             "is used."
        }
        form method="POST" {
            div class="mb-3" {
                label for="title" class="form-label" {
                    "Title"
                }
                input name="title" type="text" class="form-control" id="title" {}
            }
            div class="mb-3" {
                label for="description" class="form-label" {
                    "Description"
                }
                textarea name="description" type="text" class="form-control" id="description" {}
            }
            button type="submit" class="btn btn-primary" { "Submit" }
        }
    }
}

#[derive(FromForm, Serialize, Debug)]
pub struct CreateSparSeriesForm {
    pub title: String,
    pub description: Option<String>,
}

#[post("/groups/<group_id>/spar_series/new", data = "<form>")]
pub async fn do_create_new_spar_series(
    group_id: String,
    user: User,
    db: DbConn,
    form: Form<CreateSparSeriesForm>,
) -> Option<Result<Flash<Redirect>, Markup>> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let group = match groups::table
                .filter(groups::public_id.eq(group_id.clone()))
                .get_result::<Group>(conn)
                .optional()?
            {
                Some(group) => group,
                None => return Ok(None),
            };

            let has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef(group.id)),
                conn,
            );
            if !has_permission {
                return Ok(Some(Ok(Flash::error(
                    Redirect::to("/"),
                    "Error: you do not have permission to do that!",
                ))));
            }

            let public_id = gen_uuid().to_string();

            let already_exists = select(exists(
                spar_series::table
                    .filter(spar_series::group_id.eq(group.id))
                    .filter(spar_series::title.eq(&form.title)),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if already_exists {
                return Ok(Some(Err(page_of_body(html! {
                    div class="alert alert-danger" role="alert" {
                        "Error: a spar series with that name already exists. Please pick a new name!"
                    }
                    (make_new_spar_series_form())
                }, Some(user)))));
            }

            let uuid = insert_into(spar_series::table)
                .values((
                    spar_series::public_id.eq(public_id),
                    spar_series::title.eq(&form.title),
                    spar_series::description.eq(&form.description),
                    // todo: add support for other formats, and then change this
                    spar_series::speakers_per_team.eq(2),
                    spar_series::group_id.eq(group.id),
                    spar_series::created_at.eq(Utc::now().naive_utc()),
                    spar_series::allow_join_requests.eq(true),
                    spar_series::auto_approve_join_requests.eq(false),
                ))
                .returning(spar_series::public_id)
                .get_result::<String>(conn)
                .unwrap();

            Ok(Some(Ok(Flash::success(
                Redirect::to(format!("/spar_series/{}", uuid)),
                "Created that internal!",
            ))))
        })
        .unwrap()
    })
    .await
}
