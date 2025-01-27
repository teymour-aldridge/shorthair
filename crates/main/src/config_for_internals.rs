use chrono::{NaiveDateTime, Utc};
use db::{
    group::Group,
    schema::{group_members, groups, spar_series, spars},
    spar::{Spar, SparSeries},
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
use serde::Serialize;
use uuid::Uuid;

use crate::html::page_of_body;

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
                    @for session in &sessions {
                        div class="card" style="width: 18rem;" {
                            div class="card-body" {
                                h5 class="card-title" { "Session of " span class="render-date" { (session.start_time.format("%Y-%m-%d %H:%M:%S")) } }
                                a href=(format!("/spars/{}", session.public_id)) class="card-link" { "View session" }
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

                let public_id = insert_into(spars::table)
                    .values((
                        spars::public_id.eq(Uuid::now_v7().to_string()),
                        spars::spar_series_id.eq(spar_series.id),
                        spars::created_at.eq(Utc::now().naive_utc()),
                        spars::is_open.eq(form.is_open.is_some()),
                        spars::release_draw.eq(false),
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
