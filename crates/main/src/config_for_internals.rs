/// This file contains code to allow group administrators to manage internal
/// spars (for example, creating new spars, marking spars as complete, etc).
use chrono::{NaiveDateTime, Utc};
use db::{
    group::{Group, GroupMember},
    schema::{
        group_members, groups, spar_series, spar_series_join_requests,
        spar_series_members, spars,
    },
    spar::{Spar, SparSeries, SparSeriesMember},
    user::User,
    DbConn,
};
use diesel::{
    dsl::{exists, insert_into, select},
    prelude::*,
};
use maud::{html, Markup};
use rocket::{
    form::Form,
    response::{status::Unauthorized, Redirect},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    html::{error_403, page_of_body},
    model::sync::id::gen_uuid,
    util::is_valid_email,
};

#[get("/spar_series/<internal_id>")]
/// Displays an overview of the current internals.
pub async fn internal_page(
    internal_id: String,
    user: User,
    db: DbConn,
) -> Option<Result<Markup, Unauthorized<()>>> {
    db.run(|conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let t = spar_series::table
                .filter(spar_series::public_id.eq(internal_id))
                .inner_join(groups::table)
                .get_result::<(SparSeries, Group)>(conn)
                .optional()
                .unwrap();

            if let Some((spar_series, group)) = t {
                let user_is_admin = {
                    let select = select(exists(
                        group_members::table
                            .filter(group_members::group_id.eq(group.id))
                            .filter(group_members::user_id.eq(user.id))
                            .filter(
                                group_members::has_signing_power
                                    .eq(true)
                                    .or(group_members::is_admin.eq(true)),
                            ),
                    ));
                    select.get_result::<bool>(conn)?
                };

                if !user_is_admin {
                    return Ok(Some(Err(Unauthorized(()))));
                }

                let sessions = spars::table
                    .filter(spars::spar_series_id.eq(spar_series.id))
                    .load::<Spar>(conn)?;

                let markup = html! {
                    h1 { "Internal " (spar_series.title) }
                    @if let Some(description) = &spar_series.description {
                        p { (description) }
                    }
                    a href=(format!("/spar_series/{}/makesess", spar_series.public_id)) type="button" class="btn btn-primary" { "Create new session" }
                    table class="table" {
                        thead {
                            tr {
                                th scope="col" {"#"}
                                th scope="col" {"Date"}
                                th scope="col" {"Is complete"}
                                th scope="col" {"Link to session"}
                            }
                        }
                        tbody {
                            @for (i, spar) in sessions.iter().enumerate() {
                                tr {
                                    th scope="row" {
                                        (i)
                                    }
                                    td {
                                        (spar.start_time.format("%Y-%m-%d %H:%M:%S"))
                                    }
                                    @if spar.is_complete {
                                        td class="table-success" {
                                            (spar.is_complete)
                                        }
                                    } else {
                                        td class="table-danger" {
                                            (spar.is_complete)
                                        }
                                    }
                                    td {
                                        a href=(format!("/spars/{}", spar.public_id)) class="card-link" { "View session" }
                                    }
                                }
                            }
                        }
                    }
                };

                Ok(Some(Ok(page_of_body(markup, Some(user)))))
            } else {
                Ok(None)
            }
        })
        .unwrap()
    })
    .await
}

#[get("/spar_series/<internal_id>/makesess")]
/// Create a new session page.
pub async fn make_session_page(
    internal_id: String,
    user: User,
    db: DbConn,
) -> Option<Result<Markup, Unauthorized<()>>> {
    db.run(|conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let t = spar_series::table
                .filter(spar_series::public_id.eq(internal_id))
                .inner_join(groups::table)
                .get_result::<(SparSeries, Group)>(conn)
                .optional()
                .unwrap();

            if let Some((_, group)) = t {
                let user_is_admin = select(exists(
                    group_members::table
                        .filter(group_members::group_id.eq(group.id))
                        .filter(group_members::user_id.eq(user.id))
                        .filter(
                            group_members::has_signing_power
                                .eq(true)
                                .or(group_members::is_admin.eq(true)),
                        ),
                ))
                .get_result::<bool>(conn)
                .unwrap();

                if !user_is_admin {
                    return Ok(Some(Err(Unauthorized(()))));
                }

                let markup = create_spar_form(None);

                Ok(Some(Ok(page_of_body(markup, Some(user)))))
            } else {
                Ok(None)
            }
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize, Debug)]
pub struct MakeSessionForm {
    pub start_time: String,
    pub is_open: Option<String>,
}

#[post("/spar_series/<internal_id>/makesess", data = "<form>")]
/// Create a new internals session.
pub async fn do_make_session(
    internal_id: String,
    user: User,
    db: DbConn,
    form: Form<MakeSessionForm>,
) -> Option<Result<Result<Markup, Redirect>, Unauthorized<()>>> {
    db.run(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(move |conn| {
            let t = spar_series::table
                .filter(spar_series::public_id.eq(internal_id))
                .inner_join(groups::table)
                .get_result::<(SparSeries, Group)>(conn)
                .optional()
                .unwrap();

            if let Some((internal, institution)) = t {
                let (spar_series, group): (SparSeries, Group) =
                    (internal, institution);
                let user_is_admin = select(exists(
                    group_members::table
                        .filter(group_members::group_id.eq(group.id))
                        .filter(group_members::user_id.eq(user.id))
                        .filter(
                            group_members::has_signing_power
                                .eq(true)
                                .or(group_members::is_admin.eq(true)),
                        ),
                ))
                .get_result::<bool>(conn)
                .unwrap();

                if !user_is_admin {
                    return Ok(Some(Err(Unauthorized(()))));
                }

                let start_time = match NaiveDateTime::parse_from_str(
                    &form.start_time,
                    "%Y-%m-%dT%H:%M",
                ) {
                    Ok(s) => s,
                    Err(_) => {
                        dbg!(&form.start_time);
                        let markup = create_spar_form(Some(
                            "The provided start time is not a valid date."
                                .to_string(),
                        ));
                        return Ok(Some(Ok(Ok(page_of_body(
                            markup,
                            Some(user),
                        )))));
                    }
                };

                let previous_spars_complete =
                    diesel::dsl::select(diesel::dsl::exists(
                        spars::table
                            .filter(spars::spar_series_id.eq(spar_series.id))
                            .filter(spars::is_complete.eq(false)),
                    ))
                    .get_result::<bool>(conn)?;

                let count = spars::table
                    .filter(spars::spar_series_id.eq(spar_series.id))
                    .count()
                    .get_result::<i64>(conn)?;

                if !previous_spars_complete && (count > 0) {
                    let markup = create_spar_form(Some(
                        "Error: all previous spars must be marked as complete
                         before starting a new spar."
                            .to_string(),
                    ));
                    return Ok(Some(Ok(Ok(page_of_body(markup, Some(user))))));
                }

                let public_id = insert_into(spars::table)
                    .values((
                        spars::public_id.eq(gen_uuid().to_string()),
                        spars::spar_series_id.eq(spar_series.id),
                        spars::created_at.eq(Utc::now().naive_utc()),
                        spars::is_open.eq(form.is_open.is_some()),
                        spars::release_draw.eq(false),
                        spars::is_complete.eq(false),
                        spars::start_time.eq(start_time),
                    ))
                    .returning(spars::public_id)
                    .get_result::<String>(conn)
                    .unwrap();

                return Ok(Some(Ok(Err(Redirect::to(format!(
                    "/spars/{}",
                    public_id
                ))))));
            } else {
                Ok(None)
            }
        })
        .unwrap()
    })
    .await
}

fn create_spar_form(error: Option<String>) -> Markup {
    html! {
        h1 { "Create spar" }
        @if let Some(error_msg) = error {
            div class="alert alert-danger" role="alert" {
                (error_msg)
            }
        }
        form method="POST" {
            div class="mb-3" {
                label for="start_time" class="form-label" { "Start time" }
                input name="start_time" type="datetime-local" class="form-control" id="start_time" {}
            }
            button type="submit" class="btn btn-primary" { "Submit" }
        }
    }
}

#[get("/spar_series/<internal_id>/add_member")]
pub async fn add_member_page(
    internal_id: String,
    user: User,
    db: DbConn,
) -> Markup {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let t = spar_series::table
                .filter(spar_series::public_id.eq(internal_id))
                .inner_join(groups::table)
                .get_result::<(SparSeries, Group)>(conn)
                .optional()
                .unwrap();

            if let Some((internal, institution)) = t {
                let (_spar_series, group): (SparSeries, Group) =
                    (internal, institution);
                let user_is_admin = select(exists(
                    group_members::table
                        .filter(group_members::group_id.eq(group.id))
                        .filter(group_members::user_id.eq(user.id))
                        .filter(
                            group_members::has_signing_power
                                .eq(true)
                                .or(group_members::is_admin.eq(true)),
                        ),
                ))
                .get_result::<bool>(conn)
                .unwrap();

                if !user_is_admin {
                    return Ok(error_403(
                        Some("Error: you are not authorized to administer this group.".to_string()),
                        Some(user)
                    ));
                }

                let markup = maud::html! {
                    h1 { "Add member" }
                    form method="POST" {
                        div class="mb-3" {
                            label for="name" class="form-label" { "Full name" }
                            input name="name" type="text" class="form-control" id="name" {}
                        }
                        div class="mb-3" {
                            label for="email" class="form-label" { "Email address" }
                            input name="email" type="email" class="form-control" id="email" {}
                        }
                        button type="submit" class="btn btn-primary" { "Add member" }
                    }
                };
                Ok(page_of_body(markup, Some(user)))
            } else {
                Ok(error_403(
                    Some(
                        "You are not authorized to access this group"
                            .to_string(),
                    ),
                    Some(user),
                ))
            }
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize, Deserialize)]
pub struct AddMemberForm {
    pub name: String,
    pub email: String,
}

#[post("/spar_series/<internal_id>/add_member", data = "<form>")]
pub async fn do_add_member(
    internal_id: String,
    user: User,
    db: DbConn,
    form: Form<AddMemberForm>,
) -> Markup {
    db.run(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(move |conn| {
            let (spar_series, group) = spar_series::table
                .filter(spar_series::public_id.eq(&internal_id))
                .inner_join(groups::table)
                .get_result::<(SparSeries, Group)>(conn)?;

            let user_is_admin = select(exists(
                group_members::table
                    .filter(group_members::group_id.eq(group.id))
                    .filter(group_members::user_id.eq(user.id))
                    .filter(
                        group_members::has_signing_power
                            .eq(true)
                            .or(group_members::is_admin.eq(true)),
                    ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if !user_is_admin {
                return Ok(error_403(
                    Some("Error: you are not authorized to administer this group.".to_string()),
                    Some(user)
                ));
            }

            let n = insert_into(spar_series_members::table)
                .values((
                    spar_series_members::public_id.eq(gen_uuid().to_string()),
                    spar_series_members::name.eq(&form.name),
                    spar_series_members::email.eq(&form.email),
                    spar_series_members::spar_series_id.eq(spar_series.id),
                    spar_series_members::created_at.eq(Utc::now().naive_utc()),
                ))
                .execute(conn)?;

            assert_eq!(n, 1);


            let markup = html! {
                h1 { "Member added successfully" }
                p { "Added " (form.name) " to the spar series." }
                a href=(format!("/spar_series/{}", internal_id)) { "Return to spar series" }
            };

            Ok(page_of_body(markup, Some(user)))
        })
        .unwrap()
    })
    .await
}

fn render_request2join_form(error: Option<String>) -> Markup {
    html! {
        h1 { "Request to Join" }
        @if let Some(error_msg) = error {
            div class="alert alert-danger" role="alert" {
                (error_msg)
            }
        }
        form method="POST" {
            div class="mb-3" {
                label for="name" class="form-label" { "Full name" }
                input name="name" type="text" class="form-control" id="name" {}
            }
            div class="mb-3" {
                label for="email" class="form-label" { "Email address" }
                input name="email" type="email" class="form-control" id="email" {}
            }
            button type="submit" class="btn btn-primary" { "Submit request" }
        }
    }
}

#[get("/spar_series/<spar_series_id>/request2join")]
pub async fn request2join_spar_series_page(
    spar_series_id: String,
    db: DbConn,
    user: Option<User>,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let series = match spar_series::table
                .filter(spar_series::public_id.eq(&spar_series_id))
                .first::<SparSeries>(conn)
                .optional()
                .unwrap()
            {
                Some(series) => series,
                None => return Ok(None),
            };
            // shouldn't be possible to have join requests disabled and auto
            // approve join requests enabled (illogical)
            assert!(!(!series.allow_join_requests && series.auto_approve_join_requests));

            if !series.allow_join_requests {
                return Ok(Some(html! {
                    h1  {
                        "Error: join requests are not enabled for this spar series."
                    }
                }))
            }

            let page = render_request2join_form(None);
            Ok(Some(page_of_body(page, user)))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize)]
pub struct Request2JoinSparSeriesForm {
    pub(crate) name: String,
    pub(crate) email: String,
}

#[post("/spar_series/<spar_series_id>/request2join", data = "<form>")]
pub async fn do_request2join_spar_series(
    spar_series_id: String,
    db: DbConn,
    user: Option<User>,
    form: Form<Request2JoinSparSeriesForm>,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !is_valid_email(dbg!(&form.email)) {
                let page = render_request2join_form(Some(
                    "Error: invalid email supplied.".to_string(),
                ));
                return Ok(Some(page_of_body(page, user)));
            }

            let series = match spar_series::table
                .filter(spar_series::public_id.eq(spar_series_id))
                .first::<SparSeries>(conn)
                .optional()
                .unwrap()
            {
                Some(series) => series,
                None => return Ok(None),
            };

            if !series.allow_join_requests {
                // todo: standardise the error message
                return Ok(Some(page_of_body(
                    html! {h1 {"Error: join requests are closed for this spar"}},
                    user,
                )));
            }

            let existing_member = spar_series_members::table
                .filter(spar_series_members::name.eq(&form.name))
                .or_filter(spar_series_members::email.eq(&form.email))
                .first::<SparSeriesMember>(conn)
                .optional()
                .unwrap();
            if existing_member.is_some() {
                let body = render_request2join_form(Some("Such a user already exists".to_string()));
                return Ok(Some(page_of_body(body, user)))
            }

            if series.auto_approve_join_requests {
                let n_inserted = insert_into(spar_series_members::table).values((
                    spar_series_members::public_id.eq(gen_uuid().to_string()),
                    spar_series_members::name.eq(&form.name),
                    spar_series_members::email.eq(&form.email),
                    spar_series_members::spar_series_id.eq(series.id),
                    spar_series_members::created_at.eq(diesel::dsl::now),
                ))
                .execute(conn).unwrap();
                assert_eq!(n_inserted, 1);

                let body = html! {
                    h1 {"You are now a member of this spar series. Please return
                        to the spar and sign up!"}
                };
                Ok(Some(page_of_body(body, user)))
            } else {
                let n_inserted = insert_into(spar_series_join_requests::table).values((
                    spar_series_join_requests::public_id.eq(gen_uuid().to_string()),
                    spar_series_join_requests::name.eq(&form.name),
                    spar_series_join_requests::email.eq(&form.email),
                    spar_series_join_requests::spar_series_id.eq(series.id),
                    spar_series_join_requests::created_at.eq(diesel::dsl::now),
                ))
                .execute(conn).unwrap();
                assert_eq!(n_inserted, 1);

                let body = html! {
                    h1 {"Your request to join this spar series has been recorded."}
                    h3 {"Please talk to the administrator of this spar and ask them
                        to approve you!"}
                };
                Ok(Some(page_of_body(body, user)))
            }
        })
        .unwrap()
    })
    .await
}

#[get("/spar_series/<spar_series_id>/join_requests")]
pub async fn join_requests_page(
    db: DbConn,
    spar_series_id: String,
    user: User,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let series = match spar_series::table
                .filter(spar_series::public_id.eq(&spar_series_id))
                .first::<SparSeries>(conn)
                .optional()
                .unwrap()
            {
                Some(series) => series,
                None => return Ok(None),
            };

            match group_members::table
                .filter(group_members::group_id.eq(series.group_id))
                .filter(group_members::user_id.eq(user.id))
                .first::<GroupMember>(conn)
                .optional()
                .unwrap()
            {
                Some(membership)
                    if membership.has_signing_power_bool
                        | membership.is_admin =>
                {
                    ()
                }
                Some(_) | None => {
                    return Ok(Some(error_403(
                        Some(
                            "Error: you are not authorized to view this group!",
                        ),
                        Some(user),
                    )))
                }
            };

            let join_requests = spar_series_join_requests::table
                .filter(spar_series_join_requests::spar_series_id.eq(series.id))
                .order_by(spar_series_join_requests::created_at.asc())
                .load::<SparSeriesMember>(conn)
                .unwrap();

            let markup = html! {
                h1 { "Join Requests" }
                table class="table" {
                    thead {
                        tr {
                            th scope="col" { "Name" }
                            th scope="col" { "Email" }
                            th scope="col" { "Request Date" }
                            th scope="col" { "Actions" }
                        }
                    }
                    tbody {
                        @for request in join_requests {
                            tr {
                                td { (request.name) }
                                td { (request.email) }
                                td { (request.created_at.format("%Y-%m-%d %H:%M:%S")) }
                                td {
                                    input type="hidden" name="id" value=(request.public_id);
                                    button type="submit" class="btn btn-success btn-link text-decoration-none" style="color: #198754" { "Approve" }
                                }
                            }
                        }
                    }
                }
            };

            Ok(Some(page_of_body(markup, Some(user))))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize)]
pub struct ApproveJoinRequestForm {
    pub id: Uuid,
}

#[post("/spar_series/<spar_series_id>/approve_join_request", data = "<form>")]
pub async fn approve_join_request(
    db: DbConn,
    spar_series_id: String,
    user: User,
    form: Form<ApproveJoinRequestForm>,
) -> Option<Result<Redirect, Markup>> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let series = match spar_series::table
                .filter(spar_series::public_id.eq(&spar_series_id))
                .first::<SparSeries>(conn)
                .optional()
                .unwrap()
            {
                Some(series) => series,
                None => return Ok(None),
            };

            match group_members::table
                .filter(group_members::group_id.eq(series.group_id))
                .filter(group_members::user_id.eq(user.id))
                .first::<GroupMember>(conn)
                .optional()
                .unwrap()
            {
                Some(membership)
                    if membership.has_signing_power_bool
                        | membership.is_admin =>
                {
                    ()
                }
                Some(_) | None => {
                    return Ok(Some(Err(error_403(
                        Some(
                            "Error: you are not authorized to view this group!",
                        ),
                        Some(user),
                    ))))
                }
            };

            let join_request = match spar_series_join_requests::table
                .filter(
                    spar_series_join_requests::public_id
                        .eq(&form.id.to_string()),
                )
                .first::<SparSeriesMember>(conn)
                .optional()
                .unwrap()
            {
                Some(req) => req,
                None => return Ok(None),
            };

            let n_inserted = insert_into(spar_series_members::table)
                .values((
                    spar_series_members::public_id.eq(gen_uuid().to_string()),
                    spar_series_members::name.eq(join_request.name),
                    spar_series_members::email.eq(join_request.email),
                    spar_series_members::spar_series_id
                        .eq(join_request.spar_series_id),
                    spar_series_members::created_at.eq(join_request.created_at),
                ))
                .execute(conn)
                .unwrap();
            assert_eq!(n_inserted, 1);

            Ok(Some(Ok(Redirect::to(format!(
                "/spar_series/{spar_series_id}/join_requests"
            )))))
        })
        .unwrap()
    })
    .await
}
