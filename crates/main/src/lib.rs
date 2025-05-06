#![feature(coverage_attribute)]

use std::collections::HashMap;

use accounts::account_page;
use admin::{
    config::{config_page, do_upsert_config, edit_existing_config_item_page},
    invite::{
        accept_invite_page, do_accept_invite, do_invite_user, send_invite_page,
    },
    setup::{do_setup, setup_page},
};
use auth::{
    change_password::{do_set_password, profile_page},
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
    create_group_page, create_new_spar_series_page, do_create_group,
    do_create_new_spar_series, view_group,
};
use html::page_of_body;
use maud::{html, Markup};
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithHttpConfig;
use opentelemetry_sdk::{
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
    Resource,
};
use opentelemetry_semantic_conventions::{
    attribute::DEPLOYMENT_ENVIRONMENT_NAME,
    resource::{SERVICE_NAME, SERVICE_VERSION},
    SCHEMA_URL,
};
use request_ids::{RequestIdFairing, TracingSpan};
use rocket::{
    fairing::AdHoc,
    figment::{
        util::map,
        value::{Map, Value},
    },
    Build, Rocket,
};

use spar_generation::{
    allocation_problem::results::{
        results_of_spar_page, results_of_spar_series_page,
    },
    individual_spars::{
        admin_overview::{set_is_open, single_spar_overview_for_admin_page},
        draw_management::{
            confirm_draft::{confirm_draw_page, do_confirm_draw},
            draft_management::view_draft_draw,
            edit::show_draw_to_admin_page,
            generate::generate_draw,
            release::do_release_draw,
        },
        participant_overview::single_spar_overview_for_participants_page,
        signup_routes::{
            do_register_for_spar, do_spar_signup_search,
            register_for_spar_page, spar_signup_search_page,
        },
    },
    spar_series::admin_routes::{
        add_member_page, do_add_member, do_make_session, internal_page,
        make_session_page, set_member_email, set_member_email_page,
    },
};
use spar_generation::{
    ballots::{do_submit_ballot, submit_ballot_page, view_ballot},
    individual_spars::complete_spar::do_mark_spar_complete,
    spar_series::admin_routes::{
        approve_join_request, do_request2join_spar_series, join_requests_page,
        member_overview_page, request2join_spar_series_page,
        spar_series_member_overview,
    },
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
fn index(user: Option<User>, span: TracingSpan) -> maud::Markup {
    let _guard = span.0.enter();
    page_of_body(
        maud::html! {
            div {
                p { "Welcome to the index page!" }
            }
        },
        user,
    )
}

#[get("/up")]
/// This is necessary for deploying with Kamal, because it uses this URL to
/// perform a health check and requires a 200 status code (it seems strange
/// that it doesn't accept any non 5xx status code though).
pub fn up() -> Markup {
    html! {
        p {"Hello world!"}
    }
}

#[derive(Responder)]
#[response(status = 200, content_type = "js")]
pub struct JsResponse(&'static str);

#[get("/static/htmx.js")]
pub fn vendored_htmx() -> JsResponse {
    let script = include_str!("vendored/htmx.js");
    JsResponse(script)
}

#[derive(Responder)]
#[response(status = 200, content_type = "css")]
pub struct CssResponse(&'static str);

#[get("/static/styles.css")]
pub fn vendored_css() -> CssResponse {
    let stylesheet = include_str!("vendored/styles.css");
    CssResponse(stylesheet)
}

pub const MIGRATIONS: EmbeddedMigrations =
    embed_migrations!("../../migrations");

pub fn resource() -> Resource {
    Resource::builder()
        .with_schema_url(
            [
                KeyValue::new(SERVICE_NAME, env!("CARGO_PKG_NAME")),
                KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
                KeyValue::new(DEPLOYMENT_ENVIRONMENT_NAME, "develop"),
            ],
            SCHEMA_URL,
        )
        .build()
}

// Construct MeterProvider for MetricsLayer
pub fn init_meter_provider() -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
        .build()
        .unwrap();

    let reader = PeriodicReader::builder(exporter)
        .with_interval(std::time::Duration::from_secs(30))
        .build();

    // For debugging in development
    let stdout_reader = PeriodicReader::builder(
        opentelemetry_stdout::MetricExporter::default(),
    )
    .build();

    let meter_provider = MeterProviderBuilder::default()
        .with_resource(resource())
        .with_reader(reader)
        .with_reader(stdout_reader)
        .build();

    global::set_meter_provider(meter_provider.clone());

    meter_provider
}

// Construct TracerProvider for OpenTelemetryLayer
pub fn init_tracer_provider() -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_headers({
            let mut headers = HashMap::new();
            if let Ok(auth) = std::env::var("OTEL_EXPORTER_OTLP_AUTHORIZATION")
            {
                headers.insert(
                    "x-honeycomb-team".to_string(),
                    auth.parse().unwrap(),
                );
            }
            headers
        })
        .build()
        .unwrap();

    SdkTracerProvider::builder()
        // Customize sampling strategy
        .with_sampler(Sampler::ParentBased(Box::new(
            Sampler::TraceIdRatioBased(1.0),
        )))
        // If export trace to AWS X-Ray, you can use XrayIdGenerator
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource())
        .with_batch_exporter(exporter)
        .build()
}

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

    let figment = if let Ok(secret) = std::env::var("SECRET_KEY") {
        figment.merge(("secret_key", secret))
    } else {
        figment
    };

    #[allow(unexpected_cfgs)]
    let figment = if cfg!(fuzzing) {
        figment.merge(("log_level", "off"))
    } else {
        figment
    };

    #[cfg(not(test))]
    {
        use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
        use tracing_subscriber::{
            layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
        };

        use opentelemetry::trace::TracerProvider;
        let tracer_provider = init_tracer_provider();
        let meter_provider = init_meter_provider();

        let tracer = tracer_provider.tracer("tracing-otel-subscriber");

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(OpenTelemetryLayer::new(tracer))
            .with(MetricsLayer::new(meter_provider.clone()))
            .with(sentry_tracing::layer().event_filter(|md| {
                md.module_path()
                    .map(|path| {
                        if path.contains("hyper") || path.contains("rocket") {
                            sentry_tracing::EventFilter::Ignore
                        } else {
                            sentry_tracing::EventFilter::Breadcrumb
                        }
                    })
                    .unwrap_or(sentry_tracing::EventFilter::Breadcrumb)
            }))
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("trace")
                    .add_directive("hyper_util=off".parse().unwrap())
                    .add_directive("rocket=off".parse().unwrap())
                    .add_directive("hyper=off".parse().unwrap())
                    .add_directive("opentelemetry_sdk=off".parse().unwrap())
                    .add_directive("reqwest=off".parse().unwrap())
                    .add_directive("opentelemetry-otlp=off".parse().unwrap())
            }))
            .init();

        // todo: should probably shut these down rather than calling `mem::forget`
        std::mem::forget(tracer_provider);
        std::mem::forget(meter_provider);
    }

    if let Ok(sentry_url) = std::env::var("SENTRY_URL") {
        std::mem::forget(sentry::init((
            sentry_url,
            sentry::ClientOptions {
                traces_sample_rate: 0.0,
                ..sentry::ClientOptions::default()
            },
        )));

        tracing::info!("Sentry integration initialized");
    }

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
                view_group,
                create_group_page,
                do_create_group,
                create_new_spar_series_page,
                do_create_new_spar_series,
                make_session_page,
                do_make_session,
                single_spar_overview_for_admin_page,
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
                do_upsert_config,
                up,
                send_invite_page,
                do_invite_user,
                accept_invite_page,
                do_accept_invite,
                vendored_css,
                vendored_htmx,
                profile_page,
                do_set_password,
                set_member_email_page,
                set_member_email,
                confirm_draw_page,
                do_confirm_draw,
                view_draft_draw,
                generate_draw
            ],
        )
        .attach(RequestIdFairing)
}
