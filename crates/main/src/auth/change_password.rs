use std::sync::Arc;

use argon2::password_hash::PasswordHasher;
use argon2::{password_hash::SaltString, Argon2};
use argon2::{PasswordHash, PasswordVerifier};
use db::{schema::users, user::User, DbConn};
use diesel::prelude::*;
use diesel::update;
use email::send_mail;
use maud::{html, Markup};
use rand::rngs::OsRng;
use rocket::{form::Form, response::Redirect};
use tracing::Instrument;

use crate::html::page_of_body;
use crate::request_ids::TracingSpan;

fn auth_profile_page(user: Option<User>, error: Option<String>) -> Markup {
    page_of_body(
        html! {
            h1 class="mb-4" { "Change Your Password" }

            form action="/user/setpassword" method="POST" {
                div class="mb-3" {
                    label for="old_password" class="form-label" { "Current Password:" }
                    input type="password" class="form-control" id="old_password" name="old_password" required;
                }

                div class="mb-3" {
                    label for="password" class="form-label" { "New Password:" }
                    input type="password" class="form-control" id="password" name="password" required;
                }

                div class="mb-3" {
                    label for="password2" class="form-label" { "Confirm New Password:" }
                    input type="password" class="form-control" id="password2" name="password2" required;
                }

                @if let Some(error_msg) = error {
                    div class="alert alert-danger" role="alert" {
                        (error_msg)
                    }
                }

                button type="submit" class="btn btn-primary" { "Change Password" }
            }
        },
        user,
    )
}

#[get("/user/auth")]
pub async fn profile_page(user: User) -> Markup {
    auth_profile_page(Some(user), None)
}

#[derive(FromForm)]
pub struct SetPasswordForm {
    old_password: Option<String>,
    password: String,
    password2: String,
}

/// Handles password updates for users.
#[post("/user/setpassword", data = "<form>")]
pub async fn do_set_password(
    user: User,
    db: DbConn,
    form: Form<SetPasswordForm>,
    span: TracingSpan,
) -> Result<Markup, Redirect> {
    let span1 = span.0.clone();
    let db = Arc::new(db);
    db.clone()
        .run(move |conn| {
            let _guard = span1.enter();
            conn.transaction(|conn| -> Result<_, diesel::result::Error> {
                if form.password != form.password2 {
                    return Ok(Ok(auth_profile_page(
                        Some(user),
                        Some("Those passwords do not match!".to_string()),
                    )));
                }

                let salt = SaltString::generate(&mut OsRng);

                let argon2 = Argon2::default();
                let new_password_hash = argon2
                    .hash_password(form.password.as_bytes(), &salt)
                    .unwrap()
                    .to_string();

                if form.old_password.is_none() {
                    return Ok(Ok(auth_profile_page(
                        Some(user),
                        Some(
                            "You have not specified the old password."
                                .to_string(),
                        ),
                    )));
                }

                let pwdhash = &user.password_hash;
                let old_password_matches = argon2
                    .verify_password(
                        form.old_password.as_ref().unwrap().as_bytes(),
                        &PasswordHash::new(&pwdhash).unwrap(),
                    )
                    .is_ok();

                if !old_password_matches {
                    return Ok(Ok(auth_profile_page(
                        Some(user),
                        Some(
                            "The provided old password does not match your
                                  actual, current, password."
                                .to_string(),
                        ),
                    )));
                }

                send_mail(
                    vec![(
                        &user.username.unwrap_or(user.email.clone()),
                        &user.email,
                    )],
                    "Eldemite.net password change",
                    &maud::html! {
                        p {
                            "Your password was just changed for eldemite.net. If
                             this was not you, please email "
                             a href="mailto:teymour@reasoning.page" {
                                 "teymour@reasoning.page"
                             }
                             " as soon as possible."
                        }
                    }
                    .into_string(),
                    "Your password was just changed for eldemite.net. If \
                 this was not you, please email teymour@reasoning.page.",
                    db.clone(),
                );

                let n = update(users::table)
                    .filter(users::id.eq(user.id))
                    .set((users::password_hash.eq(new_password_hash),))
                    .execute(conn)
                    .unwrap();
                assert_eq!(n, 1);

                Ok(Err(Redirect::to("/profile/auth")))
            })
            .unwrap()
        })
        .instrument(span.0)
        .await
}
