use std::sync::Arc;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::Utc;
use db::{
    magic_link::MagicLink,
    schema::{
        magic_links::{self, already_used},
        users,
    },
    user::{set_login_cookie, User},
    DbConn,
};
use diesel::select;
use diesel::{dsl::exists, prelude::*};
use email::send_mail;
use rocket::{
    form::Form,
    http::CookieJar,
    response::{Flash, Redirect},
};
use serde::Serialize;

use crate::{html::page_of_body, util::short_random};

#[get("/login")]
pub async fn login_with_password(
    user: Option<User>,
) -> Result<maud::Markup, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "You are already logged in!",
        ));
    }

    let markup = login_with_password_form(None);
    Ok(page_of_body(markup, user))
}

#[derive(FromForm, Serialize, Debug)]
pub struct PasswordLoginForm {
    pub email: String,
    pub password: String,
}

#[post("/login", data = "<form>")]
pub async fn do_password_login(
    user: Option<User>,
    form: Form<PasswordLoginForm>,
    jar: &CookieJar<'_>,
    db: DbConn,
) -> Result<maud::Markup, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "You are already logged in!",
        ));
    }

    let (ret, set_cookie) = db
        .run(move |conn| {
            let user: Option<User> = users::table
                .filter(users::email.eq(&form.email))
                .first::<User>(conn)
                .optional()
                .unwrap();

            match user {
                Some(user) => {
                    let parsed_hash =
                        PasswordHash::new(&user.password_hash).unwrap();
                    if Argon2::default()
                        .verify_password(form.password.as_bytes(), &parsed_hash)
                        .is_ok()
                    {
                        return (
                            Err(Flash::new(
                                Redirect::to("/login/sans_password"),
                                "info",
                                "You are now logged in.",
                            )),
                            Some(user.id),
                        );
                    } else {
                        return (
                            Ok(page_of_body(
                                login_with_password_form(Some(
                                    "Incorrect password.".to_string(),
                                )),
                                Some(user),
                            )),
                            None,
                        );
                    }
                }
                None => {
                    return (
                        Ok(page_of_body(
                            login_with_password_form(Some(
                                "No such user".to_string(),
                            )),
                            user,
                        )),
                        None,
                    )
                }
            }
        })
        .await;

    if let Some(cookie) = set_cookie {
        set_login_cookie(cookie, jar);
    }
    ret
}

fn login_with_password_form(error: Option<String>) -> maud::Markup {
    maud::html! {
        div class="container" {
            @if let Some(err) = error {
                div class="alert alert-danger" {
                    (err)
                }
            }
            form method="post" {
                div class="form-group" {
                    label for="email" { "Email address" }
                    input type="email" class="form-control" id="email" name="email" placeholder="Enter email";
                }
                div class="form-group" {
                    label for="password" { "Password" }
                    input type="password" class="form-control" id="password" name="password" placeholder="Password";
                }
                button type="submit" class="btn btn-primary" { "Submit" }
                p {
                    "If you prefer, you can "
                    a href="/register" {
                        "register for a new account "
                    }
                    a href="/login/sans_password" {
                        "or log in via email"
                    }
                    "."
                }
            }
        }
    }
}

#[get("/login/sans_password")]
pub async fn login_page(
    user: Option<User>,
) -> Result<maud::Markup, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "You are already logged in!",
        ));
    }

    let markup = maud::html! {
        h1 { "Login" }
        form method="POST" {
            div class="mb-3" {
                label for="email" class="form-label" { "Email" }
                input name="email" type="text" class="form-control" id="username";
            }
            button type="submit" class="btn btn-primary" { "Submit" }
        }
    };

    Ok(page_of_body(markup, user))
}

#[derive(FromForm)]
pub struct LoginForm {
    email: String,
}

#[post("/login/sans_password", data = "<login>")]
pub async fn do_login(
    login: Form<LoginForm>,
    db: DbConn,
    user: Option<User>,
) -> Result<Redirect, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "Error: you are already logged in!",
        ));
    };

    let db = Arc::new(db);

    let response = db
        .clone()
        .run(move |conn| {
            conn.transaction(|conn| -> Result<_, diesel::result::Error> {
                let user: Option<User> = users::table
                    .filter(users::email.eq(login.email.clone()))
                    .first(conn)
                    .optional()
                    .unwrap();

                let user = if let Some(u) = user {
                    u
                } else {
                    return Ok(Err(Flash::error(
                        Redirect::to("/register"),
                        "Please register first!",
                    )));
                };

                let code = short_random(32);
                let now = Utc::now().naive_utc();
                let expiry = now + chrono::Duration::minutes(30);
                let n = diesel::insert_into(magic_links::table)
                    .values((
                        magic_links::code.eq(&code),
                        magic_links::user_id.eq(user.id),
                        magic_links::created_at.eq(now),
                        magic_links::expires_at.eq(expiry),
                        already_used.eq(false),
                    ))
                    .execute(conn)
                    .unwrap();
                assert_eq!(n, 1);

                let name =
                    user.username.unwrap_or("[unnamed user]".to_string());
                // todo: the setup view should allow users to configure the link used for
                // this site
                //
                // (to avoid hitting the database we can also consider using a global
                //  RwLock to store this location)
                let login_code =
                    format!("https://eldemite.net/login/code/{code}");

                let html = maud::html! {
                    p { "Dear " (name) "," }
                    p {
                        "Please use this link to login to eldemite.net"
                        a href = (login_code) { (login_code) }
                    }
                };

                let text = format!(
                    r#"Dear {},

                    Please use this link to log in to eldemite.net

                    {}
                    "#,
                    name, login_code
                );

                send_mail(
                    vec![(&name, &user.email)],
                    "Login code for eldemite.net",
                    &html.into_string(),
                    &text,
                    db,
                );

                return Ok(Ok(Redirect::to("/login/check_email")));
            })
            .unwrap()
        })
        .await;

    response
}

#[get("/login/check_email")]
pub async fn check_email_page(user: Option<User>) -> maud::Markup {
    let markup = maud::html! {
        h1 { "Email sent" }
        p { "Please check your email!" }
    };
    page_of_body(markup, user)
}

#[get("/login/code/<code>")]
pub async fn confirm_login_with_code(
    code: String,
    user: Option<User>,
    db: DbConn,
) -> Result<maud::Markup, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "Error: you are already logged in!",
        ));
    };

    let code2 = code.clone();
    let code_exists = db
        .run(move |conn| {
            select(exists(
                magic_links::table.filter(MagicLink::valid_with_code(&code2)),
            ))
            .get_result::<bool>(conn)
            .unwrap()
        })
        .await;

    if !code_exists {
        return Err(Flash::error(Redirect::to("/"), "Error: that "));
    }

    let markup = maud::html! {
        h1 { "Confirm login" }
        form method="POST" action="/login/code" {
            input name="code" type="text" value=(code) hidden;
            button type="submit" class="btn btn-primary" { "Confirm login" }
        }
    };

    Ok(page_of_body(markup, user))
}

#[derive(FromForm)]
pub struct ConfirmLoginWithCode {
    code: String,
}

#[post("/login/code", data = "<form>")]
pub async fn do_login_with_code(
    form: Form<ConfirmLoginWithCode>,
    user: Option<User>,
    jar: &CookieJar<'_>,
    db: DbConn,
) -> Result<Redirect, Flash<Redirect>> {
    if user.is_some() {
        return Err(Flash::error(
            Redirect::to("/"),
            "Error: you are already logged in!",
        ));
    };

    let code = form.code.clone();

    let magic_link: Option<MagicLink> = db
        .run(move |conn| {
            magic_links::table
                .filter(MagicLink::valid_with_code(&code))
                .first::<MagicLink>(conn)
                .optional()
                .unwrap()
        })
        .await;

    let link = match magic_link {
        Some(link) => link,
        None => {
            return Err(Flash::error(
                Redirect::to("/"),
                "Error: no such code exists (the code may have expired or
                 already been used).",
            ))
        }
    };

    set_login_cookie(link.user_id, jar);

    return Ok(Redirect::to("/"));
}
