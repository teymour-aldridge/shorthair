use db::{
    schema::{spar_signups, spars},
    spar::{Spar, SparSignup},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use diesel::Connection;
use either::Either::{self, Right};
use maud::Markup;
use rocket::{form::Form, response::Redirect};
use serde::Serialize;
use uuid::Uuid;

use crate::html::page_of_body;

#[get("/spars/<spar_id>/signup")]
pub async fn spar_signup_page(
    spar_id: String,
    db: DbConn,
    user: User,
) -> Option<Either<Markup, Redirect>> {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar: Spar = match spars::table
                .filter(spars::public_id.eq(spar_id))
                .first::<Spar>(conn)
                .optional()?
            {
                Some(spar) => spar,
                None => return Ok(None),
            };

            if spar.release_draw {
                assert!(!spar.is_open);
                return Ok(Some(Either::Right(Redirect::to(format!("/spars/{}/viewdraw", spar.public_id.clone())))));
            }

            if !spar.is_open {
                return Ok(Some(Either::Left(page_of_body(maud::html! {
                    div class="alert alert-danger" {
                        "This spar has closed and is no longer accepting signups."
                    }
                }, Some(user)))));
            }

            let prev_signup = spar_signups::table
                .filter(spar_signups::user_id.eq(user.id))
                .filter(spar_signups::spar_id.eq(spar.id))
                .first::<SparSignup>(conn)
                .optional()?
                .map(|signup| {
                    SparSignupForm {
                        as_judge: signup.as_judge,
                        as_speaker: signup.as_speaker
                    }
                });

            Ok(Some(Either::Left(render_spar_signup_form(prev_signup, Some(user)))))
        })
        .unwrap()
    })
    .await
}

fn render_spar_signup_form(
    prev: Option<SparSignupForm>,
    user: Option<User>,
) -> Markup {
    page_of_body(
        maud::html! {
            @if prev.is_some() {
                div class="alert alert-warning" role="alert" {
                    "You have already signed up for this spar.
                     Submitting this form again will edit your status."
                }
            }
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
        },
        user,
    )
}

#[derive(FromForm, Debug, Serialize)]
pub struct SparSignupForm {
    // todo: how does Rocket parse these?
    pub as_judge: bool,
    pub as_speaker: bool,
}

#[post("/spars/<spar_id>/signup", data = "<form>")]
pub async fn do_spar_signup(
    spar_id: String,
    db: DbConn,
    user: User,
    form: Form<SparSignupForm>,
) -> Option<Either<Markup, Redirect>> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar: Spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()?
            {
                Some(spar) => spar,
                // todo: ensure Rocket renders 404 errors correctly
                None => return Ok(None),
            };

            if spar.release_draw {
                assert!(!spar.is_open);
                return Ok(
                    Some(
                        Either::Right(
                            Redirect::to(
                                format!("/spars/{}/viewdraw",
                                        spar.public_id.clone())
                            )
                        )
                    )
                );
            }

            if !spar.is_open {
                return Ok(Some(Either::Left(page_of_body(maud::html! {
                    div class="alert alert-danger" {
                        "This spar has closed and is no longer accepting signups."
                    }
                }, Some(user)))));
            }

            let prev_signup = spar_signups::table
                .filter(spar_signups::user_id.eq(user.id))
                .filter(spar_signups::spar_id.eq(spar.id))
                .first::<SparSignup>(conn)
                .optional()?;

            match prev_signup {
                Some(signup) => {
                    let n =
                        diesel::update(
                            spar_signups::table
                                .filter(spar_signups::id.eq(signup.id))
                        )
                        .set(
                            ((spar_signups::as_judge.eq(form.as_judge)),
                            (spar_signups::as_speaker.eq(form.as_speaker)))
                        )
                        .execute(conn)?;
                    assert_eq!(n, 1);
                },
                None => {
                    let n = diesel::insert_into(spar_signups::table)
                        .values(
                            (spar_signups::public_id.eq(Uuid::now_v7().to_string()),
                                spar_signups::user_id.eq(user.id),
                                spar_signups::spar_id.eq(spar.id),
                                spar_signups::as_judge.eq(form.as_judge),
                                spar_signups::as_speaker.eq(form.as_speaker))
                        ).execute(conn)?;
                    assert_eq!(n, 1);
                },
            };

            Ok(Some(Right(Redirect::to(format!("/spars/{spar_id}/signup")))))
        }).unwrap()
    })
    .await
}
