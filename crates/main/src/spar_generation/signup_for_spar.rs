//! Signup for spars.
//!
//! This is currently quite a trusting model (it assumes that people will not
//! maliciously - or accidentally - modify other people's signups).

use db::{
    schema::{spar_series, spar_series_members, spar_signups, spars},
    spar::{Spar, SparSeries, SparSeriesMember, SparSignup},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use diesel::Connection;
use maud::Markup;
use rocket::form::Form;
use serde::Serialize;

use crate::{
    html::{error_403, page_of_body},
    model::sync::id::gen_uuid,
};

#[get("/spars/<spar_id>/signup")]
pub async fn spar_signup_search_page(
    spar_id: String,
    db: DbConn,
    user: Option<User>,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional().unwrap();

            Ok(spar.map(move |spar| {
                let series = spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .first::<SparSeries>(conn)
                    .unwrap();

                let join_request_link = if series.allow_join_requests {
                    if series.auto_approve_join_requests {
                        maud::html! {
                            div class="alert alert-info" role="alert" {
                                "If you are not a member of this spar group"
                                a href=(format!("/spar_series/{}/request2join", series.public_id)) {
                                    "you can join on this page."
                                }
                            }
                        }
                    } else {
                        maud::html! {
                            div class="alert alert-info" role="alert" {
                                "If you are not a member of this series you can "
                                a href=(format!("/spar_series/{}/request2join", series.public_id)) {
                                    "request to join on this page."
                                }
                            }
                        }
                    }
                } else {
                    maud::html!()
                };

                let markup = maud::html! {
                    (join_request_link)
                    form method="post" {
                        div class="mb-3" {
                            label for="query" class="form-label" {
                                "Name"
                            }
                            input type="text" class="form-control" id="query" name="query" aria-describedby="queryHelp" {}
                            div id="queryHelp" class="form-text" {
                                "This is not case sensitive."
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Search" }
                    }
                };

                page_of_body(markup, user)
            }))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm)]
pub struct SearchForm {
    query: String,
}

#[post("/spars/<spar_id>/signup", data = "<search>")]
pub async fn do_spar_signup_search(
    spar_id: String,
    db: DbConn,
    search: Form<SearchForm>,
    user: Option<User>,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()?;

            let query = search.query.clone();

            if let Some(spar) = spar {
                let raw_query = format!(
                    "SELECT ssm.*
                     FROM spar_series_members_fts fts
                     INNER JOIN spar_series_members ssm ON ssm.id = fts.rowid
                     WHERE ssm.spar_series_id = ? AND fts.name MATCH ?
                     ORDER BY rank"
                );

                let matches = diesel::sql_query(raw_query)
                    .bind::<diesel::sql_types::BigInt, _>(spar.spar_series_id)
                    .bind::<diesel::sql_types::Text, _>(format!("{}*", query))
                    .load::<SparSeriesMember>(conn)?;

                let markup = maud::html! {
                    form method="post" {
                        div class="mb-3" {
                            label for="query" class="form-label" {
                                "Name"
                            }
                            input type="name" class="form-control" id="query" name="query" aria-describedby="queryHelp" {}
                            div id="queryHelp" class="form-text" {
                                "This is not case sensitive."
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Search" }
                    }

                    hr {}

                    h3 {"Search results"}

                    @if matches.is_empty() {
                        h5 {"Unfortunately no results were found for that user.
                             Please ask the spar administrator to enter your
                             name into the system."}
                    } else {
                        table class="table" {
                            thead {
                                tr {
                                    th scope="col" { "Name" }
                                    th scope="col" { "Register link" }
                                }
                            }
                            tbody {
                                @for member in &matches {
                                    tr {
                                        td { (member.name) }
                                        td {
                                            a href={"/spars/" (spar_id) "/reg/" (member.public_id)} {
                                                "Register"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                };

                Ok(Some(page_of_body(markup, user)))
            } else {
                return Ok(None);
            }
        })
        .unwrap()
    })
    .await
}

#[get("/spars/<spar_id>/reg/<member_id>")]
pub async fn register_for_spar_page(
    db: DbConn,
    spar_id: String,
    member_id: String,
    user: Option<User>,
) -> Option<Markup> {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(spar_id))
                .first::<Spar>(conn)
                .optional()?;

            let member = spar_series_members::table
                .filter(spar_series_members::public_id.eq(member_id))
                .first::<SparSeriesMember>(conn)
                .optional()?;

            let (spar, member) = match (spar, member) {
                (Some(spar), Some(member)) => (spar, member),
                _ => return Ok(None),
            };

            if spar.release_draw || !spar.is_open {
                return Ok(Some(error_403(
                    Some(
                        "Error: the draw for this spar has already been
                            released (you can no longer sign up)."
                            .to_string(),
                    ),
                    user,
                )));
            }

            let prev = spar_signups::table
                .filter(spar_signups::spar_id.eq(spar.id))
                .filter(spar_signups::member_id.eq(member.id))
                .first::<SparSignup>(conn)
                .optional()?;

            let markup = maud::html! {
                form method="post" class="form" {
                    div class="mb-3" {
                        div class="form-check" {
                            input type="checkbox"
                                name="as_judge"
                                class="form-check-input"
                                id="as_judge_check"
                                checked[
                                    prev
                                        .as_ref()
                                        .map(|prev| prev.as_judge)
                                        .unwrap_or(false)
                                ];
                            label for="as_judge_check"
                                class="form-check-label" {
                                "Sign up as judge"
                            }
                        }
                    }
                    div class="mb-3" {
                        div class="form-check" {
                            input type="checkbox"
                                name="as_speaker"
                                class="form-check-input"
                                id="as_speaker_check"
                                checked[
                                    prev
                                        .as_ref()
                                        .map(|prev| prev.as_speaker)
                                        .unwrap_or(false)
                                ];
                            label for="as_speaker_check"
                                class="form-check-label" {
                                "Sign up as speaker"
                            }
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Submit" }
                }
            };

            Ok(Some(page_of_body(markup, user)))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize)]
pub struct SignupForSpar {
    pub as_judge: bool,
    pub as_speaker: bool,
}

#[post("/spars/<spar_id>/reg/<member_id>", data = "<form>")]
pub async fn do_register_for_spar(
    db: DbConn,
    spar_id: String,
    member_id: String,
    form: Form<SignupForSpar>,
    user: Option<User>,
) -> Markup {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .unwrap();

            if spar.release_draw || !spar.is_open {
                return Ok(error_403(
                    Some(
                        "Error: the draw for this spar has already been
                          released (you can no longer sign up)."
                            .to_string(),
                    ),
                    None,
                ));
            }

            let member = spar_series_members::table
                .filter(spar_series_members::public_id.eq(member_id))
                .first::<SparSeriesMember>(conn)
                .unwrap();

            diesel::insert_into(spar_signups::table)
                .values((
                    spar_signups::public_id.eq(gen_uuid().to_string()),
                    spar_signups::spar_id.eq(spar.id),
                    spar_signups::member_id.eq(member.id),
                    spar_signups::as_judge.eq(form.as_judge),
                    spar_signups::as_speaker.eq(form.as_speaker),
                ))
                .on_conflict((spar_signups::spar_id, spar_signups::member_id))
                .do_update()
                .set((
                    spar_signups::as_judge.eq(form.as_judge),
                    spar_signups::as_speaker.eq(form.as_speaker),
                ))
                .execute(conn)
                .unwrap();

            Ok(page_of_body(
                maud::html! {
                    p { "Successfully registered!" }
                },
                user,
            ))
        })
        .unwrap()
    })
    .await
}
