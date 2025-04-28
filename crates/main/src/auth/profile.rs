use argon2::password_hash::PasswordHasher;
use argon2::{password_hash::SaltString, Argon2};
use argon2::{PasswordHash, PasswordVerifier};
use db::{schema::users, user::User, DbConn};
use diesel::prelude::*;
use diesel::update;
use maud::{html, Markup};
use rand::rngs::OsRng;
use rocket::{form::Form, response::Redirect};

use crate::html::page_of_body;

fn auth_profile_page(user: Option<User>, _: Option<String>) -> Markup {
    page_of_body(html! {}, user)
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

#[post("/user/setpassword", data = "<form>")]
pub async fn do_set_password(
    user: User,
    db: DbConn,
    form: Form<SetPasswordForm>,
) -> Result<Markup, Redirect> {
    if form.password != form.password2 {
        return Ok(auth_profile_page(
            Some(user),
            Some("Those passwords do not match!".to_string()),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);

    let argon2 = Argon2::default();
    let new_password_hash = argon2
        .hash_password(form.password.as_bytes(), &salt)
        .unwrap()
        .to_string();

    if form.old_password.is_none() {
        return Ok(auth_profile_page(
            Some(user),
            Some("You have not specified the old password.".to_string()),
        ));
    }

    let pwdhash = &user.password_hash;
    let old_password_matches = argon2
        .verify_password(
            form.old_password.as_ref().unwrap().as_bytes(),
            &PasswordHash::new(&pwdhash).unwrap(),
        )
        .is_ok();

    if !old_password_matches {
        return Ok(auth_profile_page(
            Some(user),
            Some(
                "The provided old password does not match your
                      actual, current, password."
                    .to_string(),
            ),
        ));
    }

    db.run(|conn| {
        update(users::table)
            .set((users::password_hash.eq(new_password_hash),))
            .execute(conn)
            .unwrap()
    })
    .await;

    Err(Redirect::to("/profile/auth"))
}
