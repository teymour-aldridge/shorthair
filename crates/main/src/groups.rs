use chrono::Utc;
use db::{
    group::{Group, GroupMember},
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

use crate::{model::sync::id::gen_uuid, page_of_body};

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
) -> Either<Markup, Redirect> {
    db.run(move |conn| {
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
                    "Error: that name is not valid."
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
pub async fn view_groups(
    group_id: String,
    db: DbConn,
    user: Option<User>,
) -> Option<Markup> {
    db.run(|conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let query_result = {
                let group = groups::table
                    .filter(groups::public_id.eq(group_id))
                    .get_result::<Group>(conn)
                    .optional()
                    .unwrap();
                group.map(|group| {
                    let is_admin = select(exists(
                        groups::table
                            .filter(groups::id.eq(group.id))
                            .inner_join(group_members::table)
                            .filter(GroupMember::is_admin()),
                    ))
                    .get_result::<bool>(conn)
                    .unwrap();
                    let has_signing_power = select(exists(
                        groups::table
                            .filter(groups::id.eq(group.id))
                            .inner_join(group_members::table)
                            .filter(GroupMember::has_signing_power()),
                    ))
                    .get_result::<bool>(conn)
                    .unwrap();
                    (group, is_admin, has_signing_power)
                })
            };

            if let Some((group, is_admin, has_signing_power)) = query_result {
                let spar_series = spar_series::table
                    .filter(spar_series::group_id.eq(group.id))
                    .load::<SparSeries>(conn)?;

                assert!(!is_admin || has_signing_power);
                Ok(Some(page_of_body(html! {
                    h1 { "Group: " (group.name) }
                    @if is_admin {
                        h3 {"Spars"}
                        @if !spar_series.is_empty() {
                            table class="table" {
                                thead {
                                    tr {
                                        th scope="col" {"Series title"}
                                        th {
                                            "View series"
                                        }
                                    }
                                }
                                tbody {
                                    @for series in spar_series {
                                        tr {
                                            th scope="row" {
                                                (series.title)
                                            }
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

                        a class="btn btn-primary" href={ "/groups/" (group.public_id) "/spar_series/new" } { "New internal spar" }
                    }
                }, user)))
            } else {
                Ok(None)
            }
        })
        .unwrap()
    })
    .await
}

#[get("/groups/<inst_id>/spar_series/new")]
pub async fn new_internals_page(
    inst_id: String,
    user: User,
    db: DbConn,
) -> Option<Result<Markup, Flash<Redirect>>> {
    let res = db
        .run(move |conn| {
            let inst = groups::table
                .filter(groups::public_id.eq(inst_id.clone()))
                .get_result::<Group>(conn)
                .optional()
                .unwrap();
            inst.map(move |inst| {
                let auth = select(exists(
                    groups::table
                        .filter(groups::public_id.eq(inst_id))
                        .inner_join(group_members::table)
                        .filter(group_members::user_id.eq(user.id))
                        .filter(GroupMember::has_signing_power())
                        .or_filter(GroupMember::is_admin()),
                ))
                .get_result::<bool>(conn)
                .unwrap();

                (inst, auth)
            })
        })
        .await;

    res.map(|(_group, t)| {
        if t {
            Ok(page_of_body(html! {
                h1 { "Create internal spar" }
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
                    div class="mb-3" {
                        // todo: remove this (as we currently only support BP)
                        label for="speakers_per_team" class="form-label" {
                            "Speakers per team (2 for Australs, 4 for BP).

                            NOTE: Australs is not supported ."
                        }
                        input name="speakers_per_team" type="text" class="form-control" id="speakers_per_team" {}
                    }
                    button type="submit" class="btn btn-primary" { "Submit" }
                }
            }, Some(user)))
        } else {
            Err(Flash::error(
                Redirect::to("/"),
                "Error: you do not have permission to do that!",
            ))
        }
    })
}

#[derive(FromForm, Serialize, Debug)]
pub struct CreateSparSeriesForm {
    pub title: String,
    pub description: Option<String>,
    pub speakers_per_team: i64,
}

#[post("/groups/<group_id>/spar_series/new", data = "<form>")]
pub async fn do_create_spar_series(
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

            let auth = select(exists(
                groups::table
                    .filter(groups::public_id.eq(group_id))
                    .inner_join(group_members::table)
                    .filter(group_members::user_id.eq(user.id))
                    .filter(GroupMember::has_signing_power())
                    .or_filter(GroupMember::is_admin()),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if !auth {
                return Ok(Some(Ok(Flash::error(
                    Redirect::to("/"),
                    "Error: you do not have permission to do that!",
                ))));
            }

            let public_id = gen_uuid().to_string();

            let uuid = insert_into(spar_series::table)
                .values((
                    spar_series::public_id.eq(public_id),
                    spar_series::title.eq(&form.title),
                    spar_series::description.eq(&form.description),
                    spar_series::speakers_per_team.eq(form.speakers_per_team),
                    spar_series::group_id.eq(group.id),
                    spar_series::created_at.eq(Utc::now().naive_utc()),
                ))
                .returning(spar_series::public_id)
                .get_result::<String>(conn)?;

            Ok(Some(Ok(Flash::success(
                Redirect::to(format!("/spar_series/{}", uuid)),
                "Created that internal!",
            ))))
        })
        .unwrap()
    })
    .await
}
