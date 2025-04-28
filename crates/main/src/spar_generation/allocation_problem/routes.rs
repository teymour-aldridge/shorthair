use std::{collections::HashMap, sync::Arc};

use chrono::{TimeDelta, Utc};
use db::{
    ballot::{AdjudicatorBallot, AdjudicatorBallotLink, BallotRepr},
    group::Group,
    room::SparRoomRepr,
    schema::{
        adjudicator_ballots, group_members, groups,
        spar_adjudicator_ballot_links, spar_adjudicators, spar_rooms,
        spar_series, spar_series_members, spar_signups, spar_speakers,
        spar_teams, spars,
    },
    spar::{
        Spar, SparRoom, SparSeries, SparSeriesMember, SparSignup,
        SparSignupSerializer,
    },
    user::User,
    DbConn,
};

use diesel::{
    dsl::{exists, select},
    insert_into,
    prelude::*,
};
use either::Either;
use email::send_mail;
use maud::{html, Markup, PreEscaped};
use qrcode::{render::svg, EcLevel, QrCode, Version};
use rocket::response::{status::Unauthorized, Flash, Redirect};
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    html::{error_403, error_404, page_of_body},
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
};

use super::{
    ratings,
    solve_allocation::{rooms_of_speaker_assignments, solve_lp, Team},
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
) -> Option<Result<Markup, Unauthorized<()>>> {
    let spar_id = spar_id.to_string();
    db.run(move |conn| {
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

            let spar_series = spar_series::table.filter(spar_series::id.eq(spar.id))
                .first::<SparSeries>(conn)?;

            let user_has_permission = has_permission(Some(&user), &Permission::ModifyResourceInGroup(crate::resources::GroupRef(spar_series.group_id)), conn);
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
                        // todo: allow customization of the domain
                        format!("https://eldemite.net/spars/{spar_id}/signup"),
                        Version::Normal(5),
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

            Ok(Some(Ok(page_of_body(
                html! {
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
                Some(user),
            ))))
        })
        .unwrap()
    })
    .await
}

#[get("/spars/<spar_id>", rank = 2)]
pub async fn single_spar_overview_for_participants_page(
    user: Option<User>,
    db: DbConn,
    spar_id: &str,
) -> Option<Markup> {
    let spar_id = spar_id.to_string();
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(spar_id))
                .first::<Spar>(conn)
                .optional()?;

            if let Some(spar) = spar {
                if !spar.release_draw {
                    return Ok(Some(page_of_body(html! {
                        div class="alert alert-info" role="alert" {
                            h4 class="alert-heading" { "Draw Not Available" }
                            p {
                                "The draw for this spar has not yet been released by the organizers."
                            }
                        }
                    }, user)));
                }

                let draw_info: Vec<SparRoomRepr> = {
                    let spar_id = spar.id;
                    let room_ids = spar_rooms::table
                        .filter(spar_rooms::spar_id.eq(spar_id))
                        .select(spar_rooms::id)
                        .load::<i64>(conn)?;
                    room_ids
                        .iter()
                        .map(|id| SparRoomRepr::of_id(*id, conn))
                        .collect::<Result<_, diesel::result::Error>>()?
                };

                let ballots = ballots_of_rooms(&draw_info, conn)?;

                if draw_info.is_empty() {
                    Ok(Some(page_of_body(maud::html! {
                        div class="alert alert-info" {
                            b { "The draw for this spar has not been released yet." }
                        }
                    }, user)))
                } else {
                    let markup = render_draw(draw_info, ballots);
                    Ok(Some(page_of_body(markup, user)))
                }
            } else {
                Ok(None)
            }
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
) -> Option<Markup> {
    let spar_id = spar_id.to_string();
    db.run(move |conn| {
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

                let has_permission = select(exists(
                    group_members::table
                        .filter(group_members::group_id.eq(group.id))
                        .filter(group_members::user_id.eq(user.id))
                        .filter(
                            group_members::is_admin
                                .or(group_members::has_signing_power),
                        ),
                ))
                .get_result::<bool>(conn)?;

                if !has_permission {
                    // todo: return permission error
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
    .await
}

#[post("/spars/<session_id>/makedraw")]
/// Generate the draw for the internal sessions.
///
/// TODO: fix the concurrency behaviour of this code (e.g. might want a
/// ticketing system, so that users can override old draw generations if they
/// would like to)
///
/// TODO: we ideally want a way to preview the new draw before adopting it.
pub async fn generate_draw(
    user: User,
    session_id: &str,
    db: DbConn,
    span: TracingSpan,
) -> Option<Result<Flash<Redirect>, Unauthorized<()>>> {
    let session_id = session_id.to_string();
    let db = Arc::new(db);
    db.clone().run(move |conn| {
        conn.transaction(move |conn| {
            let sid = session_id.clone();
            let spar = spars::table
                .filter(spars::public_id.eq(sid))
                .get_result::<Spar>(conn)
                .optional()
                .unwrap();

            let spar = match spar {
                Some(session) => session,
                None => return Ok(None),
            };

            let user_id = user.id;
            let user_has_permission = select(exists(
                spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .inner_join(groups::table.inner_join(group_members::table))
                    .filter(group_members::user_id.eq(user_id))
                    .filter(
                        group_members::is_admin
                            .eq(true)
                            .or(group_members::has_signing_power.eq(true)),
                    ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if !user_has_permission {
                return Ok::<_, diesel::result::Error>(Some(Err(Unauthorized(()))));
            }

            let signups = Arc::new(spar_signups::table
                .filter(spar_signups::spar_id.eq(spar.id))
                .load::<SparSignup>(conn)
            ?
            .into_iter()
            .map(|signup| {
                (signup.member_id, signup)
            }).collect::<HashMap<i64, SparSignup>>());

            let n_judges = signups.iter().filter(|(_id, signup)| signup.as_judge).count();

            // todo: this section is wrong
            let _check_valid_solution_exists = {
                let n_people_only_willing_to_speak = signups
                    .iter()
                    .filter(|(_id, signup)| signup.as_speaker && !signup.as_judge)
                    .count();

                if n_people_only_willing_to_speak < 4 {
                    return Ok(Some(Ok(Flash::error(
                        Redirect::to(format!("/spars/{}", spar.public_id)),
                        "Error: too few speakers for a British Parliamentary spar (need at least 4)!",
                    ))));
                }

                // check whether in the most extreme case (where all those who are
                // willing to both speak and judge are assigned as judges) we have
                // enough people to form a debate
                if n_judges * 8 < n_people_only_willing_to_speak {
                    return Ok(Some(Ok(Flash::error(
                        Redirect::to(format!("/spars/{}", spar.id)),
                        // todo: format numbers
                        "Error: too few people willing to judge for a British
                        Parliamentary session (assuming 1 judge and 8 people)!",
                    ))));
                }
            };

            // todo: run this outside of the transaction
            let rooms = {
                let elo_scores = ratings::compute_scores(spar.spar_series_id, conn)?;
                ratings::trace_scores(&elo_scores);
                let params = solve_lp(signups.clone(), elo_scores);
                rooms_of_speaker_assignments(&params)
            };

            diesel::delete(spar_rooms::table.filter(spar_rooms::spar_id.eq(spar.id))).execute(conn)?;

            for (_, room) in rooms {
                let spar_room_id = diesel::insert_into(spar_rooms::table)
                    .values((
                        spar_rooms::public_id.eq(Uuid::now_v7().to_string()),
                        spar_rooms::spar_id.eq(spar.id)
                    ))
                    .returning(spar_rooms::id)
                    .get_result::<i64>(conn)?;

                for adj in room.panel {
                    let adj_signup = &signups[&adj];
                    diesel::insert_into(spar_adjudicators::table)
                        .values((
                            spar_adjudicators::public_id.eq(Uuid::now_v7().to_string()),
                            spar_adjudicators::member_id.eq(adj_signup.member_id),
                            spar_adjudicators::room_id.eq(spar_room_id),
                            // todo: eventually allocate chairs
                            spar_adjudicators::status.eq("panellist"),
                        ))
                        .execute(conn)?;

                    let member = spar_series_members::table.filter(spar_series_members::id.eq(adj_signup.member_id))
                        .first::<SparSeriesMember>(conn)?;

                    // todo: when deleting the records for previous rooms, we
                    // should transfer the links over to the newly instantiated
                    // rooms
                    let key = Uuid::new_v4().to_string();
                    diesel::insert_into(spar_adjudicator_ballot_links::table).values((
                        spar_adjudicator_ballot_links::public_id.eq(Uuid::now_v7().to_string()),
                        spar_adjudicator_ballot_links::link.eq(&key),
                        spar_adjudicator_ballot_links::room_id.eq(spar_room_id),
                        spar_adjudicator_ballot_links::member_id.eq(member.id),
                        spar_adjudicator_ballot_links::created_at.eq(diesel::dsl::now),
                        spar_adjudicator_ballot_links::expires_at.eq(Utc::now().naive_utc().checked_add_signed(TimeDelta::hours(5)).unwrap())
                    )).execute(conn)?;
                }

                for (team, speakers) in room.teams {
                    let position = match team {
                        Team::Og => 0,
                        Team::Oo => 1,
                        Team::Cg => 2,
                        Team::Co => 3,
                    };

                    let team_id = insert_into(spar_teams::table)
                        .values((
                            spar_teams::public_id.eq(Uuid::now_v7().to_string()),
                            spar_teams::room_id.eq(spar_room_id),
                            spar_teams::position.eq(position)
                        ))
                        .returning(spar_teams::id)
                        .get_result::<i64>(conn)?;

                    for speaker in speakers {
                        let signup = &signups[&speaker];
                        insert_into(spar_speakers::table).values((
                            spar_speakers::public_id.eq(Uuid::now_v7().to_string()),
                            spar_speakers::member_id.eq(signup.member_id),
                            spar_speakers::team_id.eq(team_id),
                        )).execute(conn)?;
                    }
                }
            }

            Ok(Some(Ok(Flash::success(
                Redirect::to(format!("/spars/{session_id}/showdraw")),
                "Draw has been created!",
            ))))
        })
    })
    .instrument(span.0)
    .await
    .unwrap()
}

#[get("/spars/<spar_id>/showdraw")]
pub async fn show_draw_to_admin_page(
    spar_id: String,
    user: User,
    db: DbConn,
) -> Markup {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(spar) => spar,
                None => {
                    return Ok(error_404(
                        Some("No such spar!".to_string()),
                        Some(user),
                    ))
                }
            };

            let user_is_admin = select(exists(
                spar_series::table
                    .filter(spar_series::id.eq(spar.id))
                    .inner_join(groups::table.inner_join(group_members::table))
                    .filter(group_members::user_id.eq(user.id))
                    .filter(
                        group_members::is_admin
                            .eq(true)
                            .or(group_members::has_signing_power.eq(true)),
                    ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            let may_view = user_is_admin || spar.release_draw;

            if !(may_view) {
                return Ok(error_403(
                    Some(
                        "Error: you don't have permission to do that"
                            .to_string(),
                    ),
                    Some(user),
                ));
            }

            let draw_info: Vec<SparRoomRepr> = {
                let spar_id = spar.id;
                let room_ids = spar_rooms::table
                    .filter(spar_rooms::spar_id.eq(spar_id))
                    .select(spar_rooms::id)
                    .load::<i64>(conn)?;
                room_ids
                    .iter()
                    .map(|id| SparRoomRepr::of_id(*id, conn))
                    .collect::<Result<_, diesel::result::Error>>()?
            };

            let ballots = ballots_of_rooms(&draw_info, conn)?;

            let markup = html! {
                @if user_is_admin {
                    form method="post" action="releasedraw" {
                        button class="btn btn-primary" type="submit" {
                            "Release draw"
                        }
                    }
                }

                (render_draw(draw_info, ballots))
            };

            Ok(page_of_body(markup, Some(user)))
        })
        .unwrap()
    })
    .await
}

// todo: make method of BallotRepr (?)
//
// or at least place in the same module
fn ballots_of_rooms(
    rooms: &[SparRoomRepr],
    conn: &mut SqliteConnection,
) -> Result<HashMap<i64, BallotRepr>, diesel::result::Error> {
    let mut ret = HashMap::with_capacity(rooms.len() * 3);
    for room in rooms {
        let ballots = adjudicator_ballots::table
            .filter(adjudicator_ballots::room_id.eq(room.inner.id))
            .load::<AdjudicatorBallot>(conn)?;
        for ballot in ballots {
            ret.insert(
                ballot.adjudicator_id,
                BallotRepr::of_id(ballot.id, conn)?,
            );
        }
    }
    Ok(ret)
}

/// Displays a draw as an HTML table.
///
/// This function takes two arguments
/// - the first (room_info) contains data describing the state of the rooms in
///   the draw
/// - the second (ballots) contains data
fn render_draw(
    room_info: Vec<SparRoomRepr>,
    ballots: HashMap<i64, BallotRepr>,
) -> Markup {
    maud::html! {
        table class="table" {
            thead {
                tr {
                    th { "Room" }
                    th { "OG" }
                    th { "OO" }
                    th { "CG" }
                    th { "CO" }
                    th { "Panel" }
                }
            }
            tbody {
                @for room in &room_info {
                    tr {
                        td { (room.inner.public_id) }
                    td {
                        @for speaker in &room.teams[0].speakers {
                            div { (room.members[&room.speakers[&speaker].member_id].name) }
                        }
                    }
                    td {
                        @for speaker in &room.teams[1].speakers {
                            div { (room.members[&room.speakers[&speaker].member_id].name) }
                        }
                    }
                    td {
                        @for speaker in &room.teams[2].speakers {
                            div { (room.members[&room.speakers[&speaker].member_id].name) }
                        }
                    }
                    td {
                        @for speaker in &room.teams[3].speakers {
                            div { (room.members[&room.speakers[&speaker].member_id].name) }
                        }
                    }
                        td {
                            @for adj in &room.judges {
                                (room.members[&adj.member_id].name.clone())
                                @if let Some(ballot) = ballots.get(&adj.id) {
                                    " ("
                                    a href=(format!("/ballots/view/{}", ballot.inner.public_id)) {
                                        "view ballot"
                                    }
                                    ")"
                                }
                            }
                        }
                    }

                }
            }
        }
    }
}

// todo: mechanism to edit draws

#[post("/spars/<spar_id>/releasedraw")]
pub async fn do_release_draw(
    spar_id: String,
    user: User,
    db: DbConn,
) -> Either<Markup, Redirect> {
    let db = Arc::new(db);
    db.clone().run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(spar) => spar,
                None => {
                    return Ok(Either::Left(error_404(
                        Some("No such spar!".to_string()),
                        Some(user),
                    )))
                }
            };

            let user_has_permission = select(exists(
                spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .inner_join(groups::table.inner_join(group_members::table))
                    .filter(group_members::user_id.eq(user.id))
                    .filter(
                        group_members::is_admin
                            .eq(true)
                            .or(group_members::has_signing_power.eq(true)),
                    ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if !user_has_permission {
                return Ok(Either::Left(error_403(
                    Some(
                        "Error: you don't have permission to do that"
                            .to_string(),
                    ),
                    Some(user),
                )));
            }

            let n = diesel::update(spars::table.filter(spars::id.eq(spar.id)))
                .set((spars::release_draw.eq(true), spars::is_open.eq(false)))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            let adjudicators = spar_adjudicator_ballot_links::table
                .inner_join(spar_rooms::table)
                .filter(spar_rooms::spar_id.eq(spar.id))
                .inner_join(spar_series_members::table)
                .select((
                    spar_adjudicator_ballot_links::all_columns,
                    spar_series_members::all_columns,
                ))
                .load::<(AdjudicatorBallotLink, SparSeriesMember)>(conn)?;

            for (adj_link, member) in adjudicators {
                let ballot_link = format!("https://eldemite.net/ballots/submit/{}", adj_link.link);
                send_mail(
                    vec![(&member.name, &member.email)],
                    "Ballot link",
                    &maud::html! {
                        "Please use " a href=(ballot_link) { "this link" } " to submit your ballot."
                    }.into_string(),
                    &format!("Please use this link to submit your ballot: {ballot_link}"),
                    db.clone()
                );
            }

            Ok(Either::Right(Redirect::to(format!("/spars/{spar_id}"))))
        })
        .unwrap()
    })
    .await
}

#[post("/spars/<spar_id>/mark_complete?<force>")]
pub async fn do_mark_spar_complete(
    spar_id: String,
    user: User,
    db: DbConn,
    // whether we should over-ride issues (e.g. missing ballots, no spar was
    // actually conducted)
    //
    // todo: should people be able to "un-mark" spars?
    force: bool,
) -> Option<Result<Redirect, Markup>> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(spar_id))
                .first::<Spar>(conn)
                .optional()?;
            let spar = match spar {
                Some(spar) => spar,
                None => return Ok(None),
            };

            let user_id = user.id;
            // todo: introduce proper permissions system
            let user_has_permission = select(exists(
                spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .inner_join(groups::table.inner_join(group_members::table))
                    .filter(group_members::user_id.eq(user_id))
                    .filter(
                        group_members::is_admin
                            .eq(true)
                            .or(group_members::has_signing_power.eq(true)),
                    ),
            ))
            .get_result::<bool>(conn)
            .unwrap();

            if !user_has_permission {
                return Ok(Some(Err(error_403(
                    Some("Error: you do not have permission to do that!"),
                    Some(user),
                ))));
            }

            if !force {
                #[derive(Debug)]
                enum Problem {
                    // todo: fix whatever this is
                    #[allow(dead_code)]
                    MissingBallots { count: usize },
                    /// We never generated a draw!
                    NoSparStarted,
                }

                let mut problems = Vec::with_capacity(2);

                let rooms_without_ballots = spar_rooms::table
                    .filter(spar_rooms::spar_id.eq(spar.id))
                    .inner_join(adjudicator_ballots::table)
                    .select(spar_rooms::all_columns)
                    .count()
                    .get_result::<i64>(conn)?;

                let total_rooms = spar_rooms::table
                    .filter(spar_rooms::spar_id.eq(spar.id))
                    .count()
                    .get_result::<i64>(conn)?;

                assert!(
                    rooms_without_ballots <= total_rooms,
                    "error: rooms_without_ballots={rooms_without_ballots} and
                            total_rooms={total_rooms}"
                );
                assert!(
                    rooms_without_ballots >= 0,
                    "rooms_without_ballots={rooms_without_ballots}"
                );

                if rooms_without_ballots > 0 {
                    problems.push(Problem::MissingBallots {
                        count: (total_rooms - rooms_without_ballots) as usize,
                    });
                }

                if total_rooms == 0 {
                    problems.push(Problem::NoSparStarted);
                }
            }

            let n = diesel::update(spars::table.filter(spars::id.eq(spar.id)))
                .set((spars::is_open.eq(false), spars::is_complete.eq(true)))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);
            Ok(Some(Ok(Redirect::to(format!("/spars/{}", spar.public_id)))))
        })
        .unwrap()
    })
    .await
}
