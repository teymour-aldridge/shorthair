use db::{
    group::Group,
    room::SparRoomRepr,
    schema::{groups, spar_rooms, spar_series, spar_signups, spars},
    spar::{Spar, SparRoom, SparSeries, SparSignup, SparSignupSerializer},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use maud::{Markup, PreEscaped};
use qrcode::{render::svg, EcLevel, QrCode};
use rocket::{request::FlashMessage, response::status::Unauthorized};
use tracing::Instrument;

use crate::{
    html::page_of_body_and_flash_msg,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    spar_generation::individual_spars::draw_management::util::{
        ballots_of_rooms, render_draw,
    },
};

#[derive(Debug, Clone, Copy, FromFormField)]
pub enum SparAdminTab {
    Draw,
    Signups,
    Settings,
}

/// Returns a box either red (if closed) or green (if open), which displays a
/// message indicating the state of the spar, and a button to toggle this state.
fn spar_signup_status(spar: &Spar) -> Markup {
    maud::html! {
        @if spar.is_open {
            div class="alert alert-success" id="sparStatus" {
                "Signups are currently "
                b { "open" }
                form
                    hx-post=(format!("/spars/{}/set_is_open?state=false", spar.public_id))
                    hx-target="#sparStatus"
                    hx-swap="outerHTML" {
                    button class="btn btn-link" type="submit" {"click to set to closed"}
                }
            }
        }
        @if !spar.is_open {
            div class="alert alert-danger" id="sparStatus" {
                "Signups are currently "
                b { "closed" }
                form
                    hx-post=(format!("/spars/{}/set_is_open?state=true", spar.public_id))
                    hx-target="#sparStatus"
                    hx-swap="outerHTML" {
                    button class="btn btn-link" type="submit" {"click to set to open"}
                }
            }
        }
    }
}

#[get("/spars/<spar_id>?<tab>")]
/// Displays the overview of a single spar for admins/those with signing power
/// for a given group.
///
/// TODO: what is the correct UI for this (e.g. signups/draw/etc)
/// maybe a tab for "draw/signups/etc" - can load data + check permissions and
/// then pick the tab to load.
pub async fn single_spar_overview_for_admin_page(
    spar_id: &str,
    db: DbConn,
    user: User,
    tab: Option<SparAdminTab>,
    msg: Option<FlashMessage<'_>>,
    span: TracingSpan,
) -> Option<Result<Markup, Unauthorized<()>>> {
    let spar_id = spar_id.to_string();
    let msg = msg.map(|msg| msg.message().to_string());
    db.run(move |conn| {
        let _guard = span.0.enter();
        conn.transaction::<_, diesel::result::Error, _>(move |conn| {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .get_result::<Spar>(conn)
                .optional()
                .unwrap();

            let spar = match spar {
                None => return Ok(None),
                Some(spar) => {
                    spar
                }
            };

            let spar_series = spar_series::table.filter(spar_series::id.eq(spar.spar_series_id))
                .first::<SparSeries>(conn).unwrap();

            let user_has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(crate::resources::GroupRef(spar_series.group_id)),
                conn
            );
            if !user_has_permission {
                rocket::info!("User with id {} unauthorized.", user.id);
                return Ok(Some(Err(Unauthorized(()))));
            }

            rocket::info!("User with id {} authorized.", user.id);

            let page = match tab {
                Some(SparAdminTab::Draw) | None => {
                    let draw = spar_rooms::table
                        .filter(spar_rooms::spar_id.eq(spar.id))
                        .load::<SparRoom>(conn)?
                        .into_iter()
                        .map(|room| SparRoomRepr::of_id(room.id, conn))
                        .collect::<Result<Vec<SparRoomRepr>, _>>()?;

                    let ballots = ballots_of_rooms(&draw, conn)?;

                    maud::html! {
                        @if !draw.is_empty() {
                            h3 {"Existing draw"}
                            (render_draw(draw, ballots))
                        }

                        form method="post" action={"/spars/"(spar.public_id)"/makedraw"} {
                            p class="text-danger" {
                                b { "WARNING: generating a draw will delete any existing draw as well as all associated data (e.g. ballots from adjudicators)" }
                            }
                            button class="btn btn-danger" type="submit" {
                                "Generate draw"
                            }
                        }
                    }
                },
                Some(SparAdminTab::Signups) => {
                    let code = QrCode::with_version(
                        format!("https://eldemite.net/spars/{spar_id}/signup"),
                        qrcode::Version::Normal(5),
                        // todo: should this be higher?
                        EcLevel::L,
                    )
                    .unwrap();

                    let qr_code = code
                        .render()
                        .min_dimensions(200, 200)
                        .dark_color(svg::Color("#ffffff"))
                        .light_color(svg::Color("#000000"))
                        .build();

                    let signups = {
                        let t = spar_signups::table
                            .filter(spar_signups::spar_id.eq(spar.id))
                            .load::<SparSignup>(conn)
                            .unwrap();
                        t.into_iter()
                            .map(|t| SparSignupSerializer::from_db_ty(t, conn))
                            .collect::<Vec<_>>()
                    };

                    maud::html! {
                        h3 { "Signups" }
                        p {
                            b {"Note: "} "new members must be entered into the
                            system before they may join. You can do so "
                            a
                                href=(format!("/spar_series/{}/add_member", spar_series.public_id)) {
                                    "on the add members page"
                                }
                                " or "
                                a href=(format!("/spar_series/{}/join_requests", spar_series.public_id)) {
                                    "approve existing members."
                                }
                        }
                        (spar_signup_status(&spar))

                        (PreEscaped(qr_code))

                        table class="table" {
                            thead {
                                tr {
                                    th scope="col" { "#" }
                                    th scope="col" { "User name" }
                                    th scope="col" { "As judge?" }
                                    th scope="col" { "As speaker?" }
                                }
                            }
                            tbody {
                                @for (i, signup) in signups.iter().enumerate() {
                                    tr {
                                        th scope="row" { (i + 1) }
                                        // todo: restrict set of allowed usernames
                                        td { (signup.member.name) }
                                        td class=(if signup.as_judge { "bg-success text-white" } else { "bg-danger text-white" }) {
                                            (signup.as_judge)
                                        }
                                        td class=(if signup.as_speaker { "bg-success text-white" } else { "bg-danger text-white" }) {
                                            (signup.as_speaker)
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(SparAdminTab::Settings) => {
                    maud::html! {
                        h3 {"Settings"}
                        p {
                            "There are currently no settings available to
                             configure (this page is here in case the
                             developers need to add settings later)."
                        }
                    }
                }
            };

            let draw_is_active = if matches!(tab, Some(SparAdminTab::Draw) | None) {"active"} else {""};
            let signups_is_active = if matches!(tab, Some(SparAdminTab::Signups)) {"active"} else {""};
            let settings_is_active = if matches!(tab, Some(SparAdminTab::Settings)) {"active"} else {""};

            Ok(Some(Ok(page_of_body_and_flash_msg(
                maud::html! {
                    div class="d-flex justify-content-between align-items-center mb-3" {
                        a href=(format!("/spar_series/{}", spar_series.public_id)) class="btn btn-secondary" {
                            "← Back to spar series"
                        }
                        @if !spar.is_complete {
                            form method="post" action=(format!("/spars/{}/mark_complete", spar.public_id)) {
                                button class="btn btn-primary" type="submit" {
                                    "Mark Complete"
                                }
                            }
                        }
                    }


                    h1 {
                        "Spar session taking place at "
                        span class="render-date" { (spar.start_time) }
                    }

                    ul class = "my-3 bg-primary-subtle nav nav-pills flex-column flex-sm-row" {
                        li class = "nav-item" {
                            a href=(format!("/spars/{spar_id}?tab=draw")) class=(format!("nav-link {draw_is_active}")) {
                                "Draw"
                            }
                        }
                        li class = "nav-item" {
                            a href=(format!("/spars/{spar_id}?tab=signups")) class=(format!("nav-link {signups_is_active}")) {
                                "Signups"
                            }
                        }
                        li class = "nav-item" {
                            a href=(format!("/spars/{spar_id}?tab=settings")) class=(format!("nav-link {settings_is_active}")) {
                                "Settings"
                            }
                        }
                        // todo: add "set complete" link
                    }

                    (page)
                },
                msg,
                Some(user),
            ))))
        })
        .unwrap()
    })
    .await
}
#[post("/spars/<spar_id>/set_is_open?<state>")]
/// Opens/closes the spar for signups.
pub async fn set_is_open(
    db: DbConn,
    user: User,
    spar_id: &str,
    state: bool,
    span: TracingSpan,
) -> Option<Markup> {
    let spar_id = spar_id.to_string();
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.clone();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()?;

            if let Some(spar) = spar {
                let group = groups::table
                    .inner_join(spar_series::table)
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .select(groups::all_columns)
                    .first::<Group>(conn)?;

                let has_permission = has_permission(
                    Some(&user),
                    &Permission::ModifyResourceInGroup(
                        crate::resources::GroupRef(group.id),
                    ),
                    conn,
                );

                if !has_permission {
                    return Ok(None);
                }

                if state {
                    diesel::update(spars::table.filter(spars::id.eq(spar.id)))
                        .set((
                            spars::is_open.eq(true),
                            spars::release_draw.eq(false),
                        ))
                        .execute(conn)?;
                } else {
                    diesel::update(spars::table.filter(spars::id.eq(spar.id)))
                        .set(spars::is_open.eq(false))
                        .execute(conn)?;
                }

                // todo: can remove this query and just update spar in-place
                let spar = spars::table
                    .filter(spars::public_id.eq(&spar_id))
                    .first::<Spar>(conn)?;

                Ok(Some(spar_signup_status(&spar)))
            } else {
                Ok(None)
            }
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}
