use argon2::PasswordHasher;
use argon2::{password_hash::SaltString, Argon2};
use db::{schema::users, DbConn};
use diesel::prelude::*;
use maud::Markup;
use rand::rngs::OsRng;
use rocket::{form::Form, response::Redirect};
use serde::Serialize;
use uuid::Uuid;

use crate::html::{error_403, page_of_body};

fn setup_page_form(error: Option<String>) -> Markup {
    page_of_body(
        maud::html! {
            @if let Some(err) = error {
                div class="alert alert-danger" role="alert" {
                    (err)
                }
            }
            form method="POST" class="container" action="/admin/setup/submit" {
                div class="mb-3" {
                    label for="username" class="form-label" { "Username" }
                    input type="text" class="form-control" id="username" name="username" required;
                }
                div class="mb-3" {
                    label for="email" class="form-label" { "Email" }
                    input type="email" class="form-control" id="email" name="email" required;
                }
                div class="mb-3" {
                    label for="password" class="form-label" { "Password" }
                    input type="password" class="form-control" id="password" name="password" required;
                }
                div class="mb-3" {
                    label for="password2" class="form-label" { "Password confirmation" }
                    input type="password2" class="form-control" id="password2" name="password2" required;
                }
                button type="submit" class="btn btn-primary" { "Create Admin Account" }
            }
        },
        None,
    )
}

#[get("/admin/setup")]
/// Page to create the first user. This is here for convenience (it would also
/// be possible to do so by manual access to the database, but this is
/// always possible).
pub async fn setup_page(db: DbConn) -> Markup {
    db.run(|conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let user_count = users::table.count().get_result::<i64>(conn)?;
            assert!(user_count >= 0);

            if user_count > 0 {
                return Ok(error_403(
                    Some(
                        "Error: setup has already been performed!".to_string(),
                    ),
                    None,
                ));
            }

            Ok(setup_page_form(None))
        })
        .unwrap()
    })
    .await
}

#[derive(FromForm, Serialize)]
pub struct SetupForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub password2: String,
}

#[post("/admin/setup", data = "<form>")]
/// Creates a superuser. This is only permitted if no users currently exist in
/// the system.
pub async fn do_setup(
    db: DbConn,
    form: Form<SetupForm>,
) -> Result<Redirect, Markup> {
    db.run(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let user_count = users::table.count().get_result::<i64>(conn)?;
            assert!(user_count >= 0);

            if user_count > 0 {
                return Ok(Err(error_403(
                    Some(
                        "Error: setup has already been performed!".to_string(),
                    ),
                    None,
                )));
            }

            if form.password != form.password2 {
                return Ok(Err(setup_page_form(Some(
                    "Error: those passwords do not match!".to_string(),
                ))));
            }

            let password = form.password.as_bytes();
            let salt = SaltString::generate(&mut OsRng);

            let argon2 = Argon2::default();

            let password_hash =
                argon2.hash_password(password, &salt).unwrap().to_string();

            let n = diesel::insert_into(users::table)
                .values((
                    users::public_id.eq(Uuid::now_v7().to_string()),
                    users::username.eq(&form.username),
                    users::password_hash.eq(password_hash),
                    users::email.eq(&form.email),
                    users::is_superuser.eq(true),
                    users::created_at.eq(diesel::dsl::now),
                ))
                .execute(conn)?;
            assert_eq!(n, 1);

            // todo: send welcome email (?)

            return Ok(Ok(Redirect::to("/admin")));
        })
        .unwrap()
    })
    .await
}
