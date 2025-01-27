use auth::login::{
    check_email_page, confirm_login_with_code, do_login, do_login_with_code,
    do_password_login, login_page, login_with_password,
};
use config_for_internals::{do_make_session, make_session_page};
use db::{user::User, DbConn};
use groups::{
    create_group_page, do_create_group, do_create_spar_series,
    new_internals_page, view_groups,
};
use html::page_of_body;
use rocket::figment::{
    util::map,
    value::{Map, Value},
};
use signup_for_spar::{do_spar_signup, spar_signup_page};
use spar_allocation::routes::{do_release_draw, generate_draw, session_page};

pub mod admin;
pub mod auth;
pub mod ballots;
pub mod compute_elo;
pub mod config_for_internals;
pub mod groups;
pub mod html;
pub mod id_gen;
pub mod model;
pub mod signup_for_spar;
pub mod spar_allocation;
pub mod util;

#[macro_use]
extern crate rocket;

#[get("/")]
fn index(user: Option<User>) -> maud::Markup {
    page_of_body(
        maud::html! {
            div {
                p { "Welcome to the index page!" }
            }
        },
        user,
    )
}

#[launch]
fn rocket() -> _ {
    let db: Map<_, Value> = map! [
    "url" => std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite.db".to_string()).into(),
    "pool_size" => 10.into(),
    "timeout" => 5.into(),
    ];

    let figment =
        rocket::Config::figment().merge(("databases", map!["database" => db]));

    // todo: add fairing to perform migrations on startup
    rocket::custom(figment).attach(DbConn::fairing()).mount(
        "/",
        routes![
            login_with_password,
            do_password_login,
            index,
            login_page,
            do_login,
            check_email_page,
            confirm_login_with_code,
            do_login_with_code,
            view_groups,
            create_group_page,
            do_create_group,
            new_internals_page,
            do_create_spar_series,
            make_session_page,
            do_make_session,
            session_page,
            generate_draw,
            do_release_draw,
            spar_signup_page,
            do_spar_signup
        ],
    )
}
