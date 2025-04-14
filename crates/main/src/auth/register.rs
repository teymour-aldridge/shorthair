use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use db::{schema::users, user::User, DbConn};
use diesel::{dsl::now, insert_into, prelude::*};
use maud::Markup;
use rand::rngs::OsRng;
use rocket::{form::Form, response::Redirect};
use serde::Serialize;

use crate::{
    html::{page_of_body, page_title},
    model::sync::id::gen_uuid,
    permissions::{has_permission, Permission},
};

#[get("/register")]
pub async fn register_page(
    user: Option<User>,
    db: DbConn,
) -> Result<Markup, Redirect> {
    if db
        .run(|conn| !has_permission(None, &Permission::RegisterAsNewUser, conn))
        .await
    {
        // todo: should return a non-200 status code
        return Ok(page_of_body(
            maud::html! {
                (page_title("Signups are currently disabled!"))
                div class="m-3" {
                    p {
                        "This site is currently in a closed invite-only beta."
                    }
                }

            },
            None,
        ));
    }

    if user.is_some() {
        // todo: add flash message
        return Err(Redirect::to("/profile"));
    }

    Ok(register_form(None, None))
}

fn register_form(form: Option<&RegisterForm>, error: Option<&str>) -> Markup {
    let markup = maud::html! {
        h1 {"register"}
        @if let Some(err) = error {
            div class="error" {
                (err)
            }
        }
        form method="post" {
            div {
                label for="username" { "Username" }
                input type="text" id="username" name="username" value=(form.map(|f| f.username.clone()).unwrap_or_default());
            }
            div {
                label for="email" { "Email" }
                input type="email" id="email" name="email" value=(form.map(|f| f.email.clone()).unwrap_or_default());
            }
            div {
                label for="password" { "Password" }
                input type="password" id="password" name="password" value=(form.map(|f| f.password.clone()).unwrap_or_default());
            }
            div {
                label for="password2" { "Confirm Password" }
                input type="password" id="password2" name="password2" value=(form.map(|f| f.password2.clone()).unwrap_or_default());
            }
            button type="submit" { "Register" }
        }
    };
    page_of_body(markup, None)
}

#[derive(FromForm, Serialize)]
pub struct RegisterForm {
    pub(crate) username: String,
    pub(crate) email: String,
    pub(crate) password: String,
    pub(crate) password2: String,
}

#[post("/register", data = "<form>")]
pub async fn do_register(
    form: Form<RegisterForm>,
    db: DbConn,
) -> Result<Redirect, Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(None, &Permission::RegisterAsNewUser, conn) {
                return Ok(Err(page_of_body(
                    maud::html! {
                        (page_title("Signups are currently disabled!"))
                        p {
                            "Please ask the system administrator to enable
                             them, or create an account for you."
                        }
                    },
                    None,
                )));
            }

            if form.password != form.password2 {
                return Ok(Err(register_form(
                    Some(&form),
                    Some("Error: your passwords do not match."),
                )));
            }

            let email = form.email.clone();

            if !User::validate_email(&email) {
                return Ok(Err(register_form(
                    Some(&form),
                    Some("Error: that email is not valid."),
                )));
            }

            if !User::validate_username(&form.username) {
                return Ok(Err(register_form(
                    Some(&form),
                    Some(
                        "Error: names should consist exclusively of letters
                         and spaces.",
                    ),
                )));
            }

            let salt = SaltString::generate(&mut OsRng);

            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(form.password.as_bytes(), &salt)
                .unwrap()
                .to_string();

            let n = insert_into(users::table)
                .values((
                    users::public_id.eq(gen_uuid().to_string()),
                    users::username.eq(&form.username),
                    users::email.eq(email),
                    users::email_verified.eq(false),
                    users::created_at.eq(now),
                    users::password_hash.eq(&password_hash),
                    users::is_superuser.eq(false),
                ))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            return Ok(Ok(Redirect::to("/profile")));
        })
        .unwrap()
    })
    .await
}
