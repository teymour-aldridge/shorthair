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
use diesel::Connection;
use diesel::{prelude::*, select};
use maud::Markup;
use rocket::form::Form;
use serde::Serialize;
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    html::{error_403, error_404, page_of_body},
    model::sync::id::gen_uuid,
    request_ids::TracingSpan,
};

#[get("/spars/<spar_id>/signup")]
pub async fn spar_signup_search_page(
    spar_id: String,
    db: DbConn,
    user: Option<User>,
    span: TracingSpan,
) -> Option<Markup> {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap();

            Ok(spar.map(move |spar| {
                let series = spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .first::<SparSeries>(conn)
                    .unwrap();

                let join_request_link = join_request_link_of_series(&series);

                render_search_form(None, user, &join_request_link, None, None)
            }))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

fn join_request_link_of_series(series: &SparSeries) -> Markup {
    if series.allow_join_requests {
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
    }
}

fn render_search_form(
    error: Option<&str>,
    user: Option<User>,
    join_request_link: &Markup,
    prev: Option<&SearchForm>,
    results: Option<Markup>,
) -> Markup {
    let markup = maud::html! {
        (join_request_link)

        @if let Some(err) = error {
            div class="alert alert-danger" role="alert" {
                (err)
            }
        }

        form method="post" {
            div class="mb-3" {
                label for="query" class="form-label" {
                    "Name"
                }
                input type="text"
                    class="form-control"
                    id="query"
                    name="query"
                    aria-describedby="queryHelp"
                    value=(prev.map(|p| p.query.clone()).unwrap_or_default()) {}
                div id="queryHelp" class="form-text" {
                    "This is not case sensitive."
                }
            }
            button type="submit" class="btn btn-primary" { "Search" }
        }

        @if let Some(r) = results {
            (r)
        }
    };

    page_of_body(markup, user)
}

#[derive(FromForm, Debug)]
pub struct SearchForm {
    query: String,
}

#[post("/spars/<spar_id>/signup", data = "<search>")]
pub async fn do_spar_signup_search(
    spar_id: String,
    db: DbConn,
    search: Form<SearchForm>,
    user: Option<User>,
    span: TracingSpan,
) -> Option<Markup> {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let (spar, spar_series) = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .inner_join(spar_series::table)
                .first::<(Spar, SparSeries)>(conn)
                .optional().unwrap()
            {
                Some((spar, spar_series)) => (spar, spar_series),
                None => return Ok(None)
            };

            let query = search.query.clone();

            if query.len() < 3 {
                return Ok(Some(render_search_form(
                    Some("Please type at least three characters!"),
                    user,
                    &maud::html!(),
                    Some(&search),
                    None,
                )))
            }

            let fts_query = query.chars().filter(|char| {
                char.is_alphanumeric() || char.is_ascii_whitespace()
            }).collect::<String>();

            let raw_query =
                r#"SELECT ssm.*
                 FROM spar_series_members_fts fts
                 INNER JOIN spar_series_members ssm ON ssm.id = fts.rowid
                 WHERE ssm.spar_series_id = ? AND fts.name MATCH (?)||'*'
                 ORDER BY rank"#;

            let matches = diesel::sql_query(raw_query)
                .bind::<diesel::sql_types::BigInt, _>(spar.spar_series_id)
                .bind::<diesel::sql_types::Text, _>(fts_query)
                .load::<SparSeriesMember>(conn).unwrap();

            let search_results = maud::html! {
                hr {}

                h3 {"Search results"}

                @if matches.is_empty() {
                    @if spar_series.allow_join_requests {
                        h5 {"Unfortunately no results were found for that user.
                             If you are not a member of this spar series, please
                             ask to join "
                             a href=(format!("/spar_series/{}/request2join", {spar_series.public_id})) {"here"}
                             "."
                        }
                    } @else {
                        h5 {"Unfortunately no results were found for that user.
                             Please ask the spar administrator to enter your
                             name into the system."}
                    }
                } @else {
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
                                        a href={"/spars/" (spar_id) "/signup/" (member.public_id)} {
                                            "Register"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            };

            Ok(Some(render_search_form(None, user, &maud::html! {}, Some(&search), Some(search_results))))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

#[get("/spars/<spar_id>/signup/<member_id>")]
pub async fn register_for_spar_page(
    db: DbConn,
    spar_id: String,
    member_id: String,
    user: Option<User>,
    span: TracingSpan,
) -> Option<Markup> {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
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
                tracing::trace!(
                    "Branch not taken (release_draw={}, is_open={})",
                    spar.release_draw,
                    spar.is_open
                );
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

            tracing::trace!("Previous spar signup is {prev:?}");

            let pick_speaking_partner = {
                let speaking_partners = spar_series_members::table
                    .filter(
                        spar_series_members::spar_series_id
                            .eq(spar.spar_series_id),
                    )
                    .load::<SparSeriesMember>(conn)
                    .unwrap();

                let prev_value = prev.as_ref().map(|t| {
                    match t.partner_preference {
                        Some(partner) => {
                            spar_series_members::table
                                .filter(
                                    spar_series_members::spar_series_id
                                        .eq(spar.spar_series_id)
                                        .and(spar_series_members::id.eq(partner)),
                                )
                                .select(spar_series_members::public_id)
                                .first::<String>(conn)
                                .unwrap()
                        },
                        None => String::new(),
                    }
                }).unwrap_or(String::new());

                maud::html! {
                    div class="mb-3" {
                        label for="speaking_partner" class="form-label" {
                            "Preferred speaking partner"
                        }
                        select class="form-select" id="speaking_partner" name="speaking_partner"  {option value="" { "None" }
                        @for partner in &speaking_partners {
                            option value=(partner.public_id) selected[partner.public_id == prev_value] {
                                (partner.name) (if partner.public_id == prev_value {" (current preference)"} else {""})
                            }
                        }
                    }
                    }
                }
            };

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
                    (pick_speaking_partner)
                    button type="submit" class="btn btn-primary" { "Submit" }
                }
            };

            Ok(Some(page_of_body(markup, user)))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

#[derive(FromForm, Serialize)]
pub struct SignupForSpar {
    pub as_judge: bool,
    pub as_speaker: bool,
    pub speaking_partner: Option<Uuid>,
}

#[post("/spars/<spar_id>/signup/<member_id>", data = "<form>")]
pub async fn do_register_for_spar(
    db: DbConn,
    spar_id: String,
    member_id: String,
    form: Form<SignupForSpar>,
    user: Option<User>,
    span: TracingSpan,
) -> Markup {
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .unwrap();

            if spar.release_draw || !spar.is_open {
                tracing::trace!("Returning as the spar has not been released");
                return Ok(error_403(
                    Some(
                        "Error: the draw for this spar has already been
                          released (you can no longer sign up)."
                            .to_string(),
                    ),
                    None,
                ));
            }

            let member = match spar_series_members::table
                .filter(spar_series_members::public_id.eq(member_id))
                .first::<SparSeriesMember>(conn)
                .optional()
                .unwrap() {
                    Some(t) => t,
                    None =>{
                        return Ok(error_404(
                            Some(
                                "No such member in spar."
                                    .to_string(),
                            ),
                            None,
                        ))
                    }
                };

            let speaking_partner_id = if let Some(partner) =
                form.speaking_partner
            {
                if !form.as_speaker {
                    return Ok(page_of_body(maud::html! {
                        div class="alert alert-danger" role="alert" {
                            "You have selected a preferred speaking partner, but
                            only signed up to judge in this spar. Please return
                            to the previous page, and either sign up as a
                            speaker, or remove your preferred speaking partner."
                        }

                    }, user))
                }

                match spar_series_members::table
                    .filter(
                        spar_series_members::public_id.eq(partner.to_string()),
                    )
                    .filter(
                        spar_series_members::spar_series_id
                            .eq(spar.spar_series_id),
                    )
                    .select(spar_series_members::id)
                    .first::<i64>(conn)
                    .optional()
                    .unwrap()
                {
                    Some(speaking_partner_id) => {
                        // todo: in addition for checking the case where you
                        // attempt to select someone who has opted to go with
                        // someone else, it might make sense to also handle the
                        // case where
                        //
                        // A and B choose to speak with each other
                        //
                        // A then switches to speak with C, meaning that we have
                        // a weird set of preferences
                        //
                        // (A -> C) and (B -> A)
                        //
                        // the correct behaviour might be to delete B -> A, and
                        // add an overview of speaker preferences on the signup
                        // page
                        if select(diesel::dsl::exists(
                            spar_signups::table
                                .filter(
                                    spar_signups::spar_id.eq(spar.id)
                                )
                                .filter(spar_signups::member_id.eq(speaking_partner_id))
                                .filter(spar_signups::partner_preference.is_not_null())
                                .filter(spar_signups::partner_preference.ne(Some(member.id))),
                        )).get_result::<bool>(conn).unwrap() {
                            return Ok(error_403(
                                Some(
                                    "The person you have signed up to speak with
                                        has selected a different speaking partner!."
                                        .to_string(),
                                ),
                                None,
                            ));

                        } else {
                            Some(speaking_partner_id)
                        }
                    }
                    None => {
                        return Ok(error_403(
                            Some(
                                "Error: you have provided an invalid speaking
                                        partner (this should never happen)."
                                    .to_string(),
                            ),
                            None,
                        ));
                    }
                }
            } else {
                None
            };

            let existing_signup = spar_signups::table
                .filter(spar_signups::spar_id.eq(spar.id))
                .filter(spar_signups::member_id.eq(member.id))
                .first::<SparSignup>(conn)
                .optional()
                .unwrap();

            let n = if let Some(existing) = existing_signup {
                diesel::update(spar_signups::table)
                    .filter(spar_signups::id.eq(existing.id))
                    .set((
                        spar_signups::as_judge.eq(form.as_judge),
                        spar_signups::as_speaker.eq(form.as_speaker),
                        spar_signups::partner_preference
                            .eq(speaking_partner_id),
                    ))
                    .execute(conn)
                    .unwrap()
            } else {
                diesel::insert_into(spar_signups::table)
                    .values((
                        spar_signups::public_id.eq(gen_uuid().to_string()),
                        spar_signups::spar_id.eq(spar.id),
                        spar_signups::member_id.eq(member.id),
                        spar_signups::as_judge.eq(form.as_judge),
                        spar_signups::as_speaker.eq(form.as_speaker),
                        spar_signups::partner_preference
                            .eq(speaking_partner_id),
                    ))
                    .execute(conn)
                    .unwrap()
            };
            assert_eq!(n, 1);

            tracing::trace!(
                "Updated user {}, set as_judge={} and as_speaker={}",
                member.id,
                form.as_judge,
                form.as_speaker
            );

            Ok(page_of_body(
                maud::html! {
                    p { "Successfully registered!" }
                },
                user,
            ))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}
