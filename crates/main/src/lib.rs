#![feature(coverage_attribute)]

use accounts::account_page;
use admin::{
    config::{config_page, do_upsert_config, edit_existing_config_item_page},
    setup::{do_setup, setup_page},
};
use auth::{
    login::{
        check_email_page, confirm_login_with_code, do_login,
        do_login_with_code, do_password_login, login_page, login_with_password,
    },
    logout,
    register::{do_register, register_page},
};
use db::{user::User, DbConn};
use diesel_migrations::{
    embed_migrations, EmbeddedMigrations, MigrationHarness,
};
use groups::{
    create_group_page, do_create_group, do_create_spar_series,
    new_internals_page, view_groups,
};
use html::page_of_body;
use request_ids::RequestIdFairing;
use rocket::{
    fairing::AdHoc,
    figment::{
        util::map,
        value::{Map, Value},
    },
    Build, Rocket,
};
use spar_generation::spar_series_routes::{
    add_member_page, approve_join_request, do_add_member, do_make_session,
    do_request2join_spar_series, internal_page, make_session_page,
    request2join_spar_series_page,
};
use spar_generation::{
    allocation_problem::{
        results::{results_of_spar_page, results_of_spar_series_page},
        routes::{
            do_mark_spar_complete, do_release_draw, generate_draw, set_is_open,
            show_draw_to_admin_page, single_spar_overview_for_admin_page,
            single_spar_overview_for_participants_page,
        },
    },
    spar_series_routes::join_requests_page,
};
use spar_generation::{
    ballots::{do_submit_ballot, submit_ballot_page, view_ballot},
    spar_series_routes::spar_series_member_overview,
};
use spar_generation::{
    signup_for_spar::{
        do_register_for_spar, do_spar_signup_search, register_for_spar_page,
        spar_signup_search_page,
    },
    spar_series_routes::member_overview_page,
};

pub mod accounts;
pub mod admin;
pub mod auth;
pub mod groups;
pub mod html;
pub mod model;
pub mod permissions;
pub mod request_ids;
pub mod resources;
pub mod spar_generation;
pub mod tests;
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

pub const MIGRATIONS: EmbeddedMigrations =
    embed_migrations!("../../migrations");

pub fn make_rocket(default_db: &str) -> Rocket<Build> {
    let db: Map<_, Value> = map![
        "url" => std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| default_db.to_string())
            .into(),
        "pool_size" => 10.into(),
        "timeout" => 5.into(),
    ];

    let figment =
        rocket::Config::figment().merge(("databases", map!["database" => db]));

    #[allow(unexpected_cfgs)]
    let figment = if cfg!(fuzzing) {
        figment.merge(("log_level", "off"))
    } else {
        figment
    };

    rocket::custom(figment)
        .attach(DbConn::fairing())
        .attach(AdHoc::try_on_ignite("migrations", |rocket| async move {
            let db_conn = DbConn::get_one(&rocket).await.unwrap();

            let ret: Result<(), Box<dyn std::error::Error + Send + Sync>> =
                db_conn
                    .run(move |conn| {
                        conn.run_pending_migrations(MIGRATIONS)?;
                        Ok(())
                    })
                    .await;

            match ret {
                Ok(_) => Ok(rocket),
                Err(_) => Err(rocket),
            }
        }))
        .mount(
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
                single_spar_overview_for_admin_page,
                generate_draw,
                do_release_draw,
                spar_signup_search_page,
                do_spar_signup_search,
                register_for_spar_page,
                do_register_for_spar,
                setup_page,
                do_setup,
                internal_page,
                add_member_page,
                do_add_member,
                set_is_open,
                show_draw_to_admin_page,
                single_spar_overview_for_participants_page,
                submit_ballot_page,
                do_submit_ballot,
                account_page,
                view_ballot,
                results_of_spar_series_page,
                results_of_spar_page,
                logout::logout,
                register_page,
                do_register,
                request2join_spar_series_page,
                do_request2join_spar_series,
                do_mark_spar_complete,
                join_requests_page,
                approve_join_request,
                spar_series_member_overview,
                member_overview_page,
                config_page,
                edit_existing_config_item_page,
                do_upsert_config
            ],
        )
        .attach(RequestIdFairing)
}
