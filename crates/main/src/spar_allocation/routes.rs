use std::{collections::HashMap, sync::Arc};

use db::{
    schema::{
        group_members, groups, spar_room_adjudicator, spar_room_team_speaker,
        spar_room_teams, spar_rooms, spar_series, spar_signups, spars, users,
    },
    spar::{Spar, SparRoom, SparSignup, SparSignupSerializer},
    user::User,
    DbConn,
};

use diesel::{
    dsl::{exists, insert_into, select},
    prelude::*,
};
use either::Either;
use maud::{html, Markup, PreEscaped};
use qrcode::{render::svg, EcLevel, QrCode, Version};
use rocket::response::{status::Unauthorized, Flash, Redirect};
use uuid::Uuid;

use crate::{
    html::{error_403, error_404, page_of_body},
    spar_allocation::solve_allocation::team_of_int,
};

use super::solve_allocation::{rooms_of_params, solve_lp, Team};

#[get("/spars/<spar_id>")]
pub async fn session_page(
    spar_id: String,
    db: DbConn,
    user: User,
) -> Option<Result<Markup, Unauthorized<()>>> {
    db.run(move |conn| {
        conn.transaction::<_, diesel::result::Error, _>(move |conn| {
            let sid = spar_id.clone();
            let session = spars::table
                .filter(spars::public_id.eq(sid))
                .get_result::<Spar>(conn)
                .optional()
                .unwrap();

            match session {
                Some(session) => {
                    let user_id = user.id;
                    let user_has_permission = select(exists(
                        spar_series::table
                            .filter(spar_series::id.eq(session.spar_series_id))
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
                        return Ok(Some(Err(Unauthorized(()))));
                    }

                    let code = QrCode::with_version(
                        format!("https://tab.reasoning.page/spars/{spar_id}/signup"),
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
                            .filter(spar_signups::spar_id.eq(session.id))
                            .load::<SparSignup>(conn)
                            .unwrap();
                        t.into_iter()
                            .map(|t| SparSignupSerializer::from_db_ty(t, conn))
                            .collect::<Vec<_>>()
                    };

                    Ok(Some(Ok(page_of_body(
                        html! {
                            h1 {
                                "Session time "
                                span class="render-date" { (session.start_time) }
                            }

                            h3 { "Signups" }
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
                                            td { (signup.user.username.clone().unwrap_or("not set".to_string())) }
                                            td { (signup.as_judge) }
                                            td { (signup.as_speaker) }
                                        }
                                    }
                                }
                            }

                            form method="post" action={"/spars/"(session.public_id)"/makedraw"} {
                                button class="btn btn-primary" type="submit" {
                                    "Generate draw"
                                }
                            }
                        },
                        Some(user),
                    ))))
                }
                None => Ok(None),
            }
        })
        .unwrap()
    })
    .await
}

#[post("/spars/<session_id>/makedraw")]
/// Generate the draw for the internal sessions.
pub async fn generate_draw(
    user: User,
    session_id: String,
    db: DbConn,
) -> Option<Result<Flash<Redirect>, Unauthorized<()>>> {
    db.run(move |conn| {
        conn.transaction(move |conn| {
            let sid = session_id.clone();
            let session = spars::table
                .filter(spars::public_id.eq(sid))
                .get_result::<Spar>(conn)
                .optional()
                .unwrap();

            let session = match session {
                Some(session) => session,
                None => return Ok(None),
            };

            let user_id = user.id;
            let user_has_permission = select(exists(
                spar_series::table
                    .filter(spar_series::id.eq(session.spar_series_id))
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

            let signups = Arc::new(
                spar_signups::table
                    .filter(spar_signups::spar_id.eq(session.id))
                    // want a consistent order
                    .order_by(spar_signups::user_id.desc())
                    .load::<SparSignup>(conn)
                    .unwrap(),
            );

            let n_judges = signups.iter().filter(|t| t.as_judge).count();
            let n_people_only_willing_to_speak = signups
                .iter()
                .filter(|t| t.as_speaker && !t.as_judge)
                .count();

            if n_people_only_willing_to_speak < 4 {
                return Ok(Some(Ok(Flash::error(
                    Redirect::to(format!("/spars/{}", session.public_id)),
                    "Error: too few speakers for a British Parliamentary spar (need at least 4)!",
                ))));
            }

            // check whether in the most extreme case (where all those who are
            // willing to both speak and judge are assigned as judges) we have
            // enough people to form a debate.
            if n_judges * 8 < n_people_only_willing_to_speak {
                return Ok(Some(Ok(Flash::error(
                    Redirect::to(format!("/spars/{}", session.id)),
                    // todo: format numbers
                    "Error: too few people willing to judge for a British
                    Parliamentary session (assuming 1 judge and 8 people)!",
                ))));
            }

            let elo_scores = HashMap::new();

            let params = solve_lp(signups.clone(), elo_scores);
            let rooms = rooms_of_params(&params);

            for (_, room) in rooms {
                let signups_ = signups.clone();
                let room_id = {
                    let id = insert_into(spar_rooms::table)
                        .values((
                            spar_rooms::public_id.eq(Uuid::now_v7().to_string()),
                            spar_rooms::spar_id.eq(session.id),
                        ))
                        .returning(spar_rooms::id)
                        .get_result::<i64>(conn)
                        .unwrap();

                    id
                };

                // first insert all the adjudicators
                let () = {
                    let mut insert = Vec::with_capacity(room.panel.len());
                    for adj_idx in room.panel {
                        let signup = &signups_.clone()[adj_idx];
                        insert.push((
                            spar_room_adjudicator::public_id.eq(Uuid::now_v7().to_string()),
                            spar_room_adjudicator::user_id.eq(signup.user_id),
                            spar_room_adjudicator::room_id.eq(room_id),
                            spar_room_adjudicator::status.eq("panelist"),
                        ));
                    }
                    let n = diesel::insert_into(spar_room_adjudicator::table)
                        .values(&insert)
                        .execute(conn)
                        .unwrap();
                    assert_eq!(n, insert.len());
                };

                // then we create all the teams
                for (i, pos) in [Team::Og, Team::Oo, Team::Cg, Team::Co].iter().enumerate() {
                    let team = room.teams[pos].clone();

                    let team_pid = Uuid::now_v7().to_string();
                    let signups2 = signups.clone();
                    let team_id = diesel::insert_into(spar_room_teams::table)
                        .values((
                            spar_room_teams::position.eq(i as i64),
                            spar_room_teams::room_id.eq(room_id),
                            spar_room_teams::public_id.eq(&team_pid),
                        ))
                        .returning(spar_room_teams::id)
                        .get_result::<i64>(conn)
                        .unwrap();

                    for speaker in team {
                        let signup = &signups2[speaker];
                        // todo: rename to spar_room_team_speakers
                        diesel::insert_into(spar_room_team_speaker::table)
                            .values((
                                spar_room_team_speaker::public_id.eq(Uuid::now_v7().to_string()),
                                spar_room_team_speaker::user_id.eq(signup.user_id),
                                spar_room_team_speaker::team_id.eq(team_id),
                            ))
                            .execute(conn)
                            .unwrap();
                    }
                }
            }

            Ok(Some(Ok(Flash::success(
                Redirect::to(format!("/spars/{session_id}/editdraw")),
                "Draw has been created!",
            ))))
        })
    })
    .await
    .unwrap()
}

pub struct SparRoomData {
    room: SparRoom,
    teams: HashMap<Team, Vec<User>>,
    panel: Vec<User>,
}

fn load_room_data(
    spar_id: i64,
    conn: &mut SqliteConnection,
) -> Result<Vec<SparRoomData>, diesel::result::Error> {
    let spar_rooms = spar_rooms::table
        .filter(spar_rooms::spar_id.eq(spar_id))
        .select(spar_rooms::all_columns)
        .load::<SparRoom>(conn)?;

    let mut spar_room_data = Vec::with_capacity(spar_rooms.len());

    for room in spar_rooms {
        let team_ids = spar_room_teams::table
            .filter(spar_room_teams::room_id.eq(room.id))
            .select((spar_room_teams::id, spar_room_teams::position))
            .load::<(i64, i64)>(conn)?;

        let mut assoc = SparRoomData {
            room,
            teams: HashMap::with_capacity(4),
            panel: Vec::new(),
        };

        for (team_id, position) in &team_ids {
            let speakers = spar_room_team_speaker::table
                .filter(spar_room_team_speaker::team_id.eq(team_id))
                .inner_join(users::table)
                .select(users::all_columns)
                .load::<User>(conn)?;
            *assoc
                .teams
                .get_mut(&team_of_int(*position as usize))
                .unwrap() = speakers;
        }

        spar_room_data.push(assoc);
    }

    Ok(spar_room_data)
}

#[get("/spars/<spar_id>/showdraw")]
pub async fn edit_draw_page(spar_id: String, user: User, db: DbConn) -> Markup {
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

            let draw_info = load_room_data(spar.id, conn)?;

            let markup = html! {
                @if user_is_admin {
                    form method="post" action="releasedraw" {
                        button class="btn btn-primary" type="submit" {
                            "Release draw"
                        }
                    }
                }

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
                        @for room in &draw_info {
                            tr {
                                td { (room.room.public_id) }
                                td {
                                    @for speaker in room.teams.get(&Team::Og).unwrap_or(&vec![]) {
                                        div { (speaker.username.clone().unwrap_or(speaker.email.clone())) }
                                    }
                                }
                                td {
                                    @for speaker in room.teams.get(&Team::Oo).unwrap_or(&vec![]) {
                                        div { (speaker.username.clone().unwrap_or(speaker.email.clone())) }
                                    }
                                }
                                td {
                                    @for speaker in room.teams.get(&Team::Cg).unwrap_or(&vec![]) {
                                        div { (speaker.username.clone().unwrap_or(speaker.email.clone())) }
                                    }
                                }
                                td {
                                    @for speaker in room.teams.get(&Team::Co).unwrap_or(&vec![]) {
                                        div { (speaker.username.clone().unwrap_or(speaker.email.clone())) }
                                    }
                                }
                                td {
                                    (room.panel.iter()
                                        .map(|adj| adj.username.clone().unwrap_or(adj.email.clone()))
                                        .collect::<Vec<_>>()
                                        .join(", "))
                                }
                            }
                        }
                    }
                }
            };

            Ok(page_of_body(markup, Some(user)))
        })
        .unwrap()
    })
    .await
}

// todo: mechanism to edit draws

#[post("/spars/<spar_id>/releasedraw")]
pub async fn do_release_draw(
    spar_id: String,
    user: User,
    db: DbConn,
) -> Either<Markup, Redirect> {
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
                    return Ok(Either::Left(error_404(
                        Some("No such spar!".to_string()),
                        Some(user),
                    )))
                }
            };

            let user_has_permission = select(exists(
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
                .set(spars::release_draw.eq(true))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            Ok(Either::Right(Redirect::to(format!(
                "/spars/{spar_id}/viewdraw"
            ))))
        })
        .unwrap()
    })
    .await
}
