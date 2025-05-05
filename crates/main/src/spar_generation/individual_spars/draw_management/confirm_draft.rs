use std::collections::HashMap;

use chrono::{TimeDelta, Utc};
use db::{
    draft_draw::DraftDraw,
    schema::{
        draft_draws, spar_adjudicator_ballot_links, spar_adjudicators,
        spar_rooms, spar_series, spar_series_members, spar_signups,
        spar_speakers,
        spar_teams::{self},
        spars,
    },
    spar::{Spar, SparSeriesMember, SparSignup},
    user::User,
    DbConn,
};
use diesel::{
    dsl::{exists, insert_into},
    prelude::*,
    select,
};
use maud::Markup;
use rocket::response::{Flash, Redirect};
use uuid::Uuid;

use crate::{
    html::page_of_body,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    resources::GroupRef,
    spar_generation::{
        allocation_problem::solve_allocation::{SolverRoom, Team},
        individual_spars::draw_management::draft_management::render_draw_data,
    },
};

#[get("/spars/<spar_id>/draws/<draw_id>/confirm")]
pub async fn confirm_draw_page(
    spar_id: &str,
    draw_id: &str,
    user: User,
    db: DbConn,
    span: TracingSpan,
) -> Option<Markup> {
    let spar_id = spar_id.to_string();
    let draw_id = draw_id.to_string();
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();

        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(s) => s,
                None => return Ok(None),
            };

            let group_id = spar_series::table
                .filter(spar_series::id.eq(spar.spar_series_id))
                .select(spar_series::group_id)
                .first::<i64>(conn)
                .unwrap();

            let has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef(group_id)),
                conn,
            );

            if !has_permission {
                // todo: could return 403 page
                return Ok(None);
            }

            let draw = match draft_draws::table
                .filter(
                    draft_draws::public_id
                        .eq(&draw_id)
                        .and(draft_draws::spar_id.eq(spar.id)),
                )
                .first::<DraftDraw>(conn)
                .optional()
                .unwrap()
            {
                Some(draw) => draw,
                None => return Ok(None),
            };

            let data: HashMap<usize, SolverRoom> = match draw.data {
                Some(data) => serde_json::from_str(&data).unwrap(),
                None => return Ok(Some(page_of_body(maud::html! {
                    p {"Cannot confirm a draw for which there is no data (please wait for draw to generate, and then refresh)."}
                }, Some(user)))),
            };

            let preexisting_draw = select(exists(
                spar_rooms::table.filter(spar_rooms::spar_id.eq(spar.id))
            ))
            .get_result::<bool>(conn);

            Ok(Some(page_of_body(
                maud::html! {
                    h1 {"Confirm draw"}
                    h3 {
                        "Would you like to confirm this draw?"
                    }
                    (render_draw_data(data, conn))

                    @if let Ok(true) = preexisting_draw {
                        div .alert.alert-danger.mt-3 {
                            h4 .alert-heading { "Warning!" }
                            p { "There is already an existing draw for this spar. If you proceed, the existing data will be overwritten and cannot be recovered." }
                            p .mb-0 { "Please make sure you want to proceed before confirming." }
                        }
                    }

                    form method="post" action={ "/spars/" (spar_id) "/draws/" (draw_id) "/confirm" } {
                        div .d-flex.mt-4 {
                            button .btn.btn-primary.me-2 type="submit" { "Confirm Draw" }
                            a .btn.btn-secondary href={ "/spars/" (spar_id) } { "Cancel" }
                        }
                    }
                },
                Some(user),
            )))
        })
        .unwrap()
    })
    .await
}

#[post("/spars/<spar_id>/draws/<draw_id>/confirm")]
pub async fn do_confirm_draw(
    spar_id: &str,
    draw_id: &str,
    user: User,
    db: DbConn,
    span: TracingSpan,
) -> Option<Flash<Redirect>> {
    let spar_id = spar_id.to_string();
    let draw_id = draw_id.to_string();
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();

        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(s) => s,
                None => return Ok(None),
            };

            let group_id = spar_series::table
                .filter(spar_series::id.eq(spar.spar_series_id))
                .select(spar_series::group_id)
                .first::<i64>(conn)
                .unwrap();

            let has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef(group_id)),
                conn,
            );

            if !has_permission {
                // todo: could return 403 page
                return Ok(None);
            }

            let draw = match draft_draws::table
                .filter(
                    draft_draws::public_id
                        .eq(&draw_id)
                        .and(draft_draws::spar_id.eq(spar.id)),
                )
                .first::<DraftDraw>(conn)
                .optional()
                .unwrap()
            {
                Some(draw) => draw,
                None => return Ok(None),
            };

            let data: HashMap<usize, SolverRoom> =
                match draw.data {
                    Some(data) => serde_json::from_str(&data).unwrap(),
                    None => return Ok(Some(Flash::error(
                        Redirect::to(format!(
                            "/spars/{spar_id}/draws/{draw_id}"
                        )),
                        "Error: could not confirm this draw, as no data exists",
                    ))),
                };

            let signups = spar_signups::table
                .filter(spar_signups::spar_id.eq(spar.id))
                .load::<SparSignup>(conn)?
                .into_iter()
                .map(|signup| (signup.member_id, signup))
                .collect::<HashMap<i64, SparSignup>>();

            diesel::delete(
                spar_rooms::table.filter(spar_rooms::spar_id.eq(spar.id)),
            )
            .execute(conn)
            .unwrap();

            for (_, room) in data {
                let spar_room_id = diesel::insert_into(spar_rooms::table)
                    .values((
                        spar_rooms::public_id.eq(Uuid::now_v7().to_string()),
                        spar_rooms::spar_id.eq(spar.id),
                    ))
                    .returning(spar_rooms::id)
                    .get_result::<i64>(conn)?;

                for adj in room.panel {
                    let adj_signup = &signups[&adj];
                    diesel::insert_into(spar_adjudicators::table)
                        .values((
                            spar_adjudicators::public_id
                                .eq(Uuid::now_v7().to_string()),
                            spar_adjudicators::member_id
                                .eq(adj_signup.member_id),
                            spar_adjudicators::room_id.eq(spar_room_id),
                            // todo: eventually allocate chairs
                            spar_adjudicators::status.eq("panellist"),
                        ))
                        .execute(conn)?;

                    let member = spar_series_members::table
                        .filter(
                            spar_series_members::id.eq(adj_signup.member_id),
                        )
                        .first::<SparSeriesMember>(conn)?;

                    // todo: when deleting the records for previous rooms, we
                    // should transfer the links over to the newly instantiated
                    // rooms
                    let key = Uuid::new_v4().to_string();
                    diesel::insert_into(spar_adjudicator_ballot_links::table)
                        .values((
                            spar_adjudicator_ballot_links::public_id
                                .eq(Uuid::now_v7().to_string()),
                            spar_adjudicator_ballot_links::link.eq(&key),
                            spar_adjudicator_ballot_links::room_id
                                .eq(spar_room_id),
                            spar_adjudicator_ballot_links::member_id
                                .eq(member.id),
                            spar_adjudicator_ballot_links::created_at
                                .eq(diesel::dsl::now),
                            spar_adjudicator_ballot_links::expires_at.eq(
                                Utc::now()
                                    .naive_utc()
                                    .checked_add_signed(TimeDelta::hours(5))
                                    .unwrap(),
                            ),
                        ))
                        .execute(conn)?;
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
                            spar_teams::public_id
                                .eq(Uuid::now_v7().to_string()),
                            spar_teams::room_id.eq(spar_room_id),
                            spar_teams::position.eq(position),
                        ))
                        .returning(spar_teams::id)
                        .get_result::<i64>(conn)?;

                    for speaker in speakers {
                        let signup = &signups[&speaker];
                        insert_into(spar_speakers::table)
                            .values((
                                spar_speakers::public_id
                                    .eq(Uuid::now_v7().to_string()),
                                spar_speakers::member_id.eq(signup.member_id),
                                spar_speakers::team_id.eq(team_id),
                            ))
                            .execute(conn)?;
                    }
                }
            }

            Ok(Some(Flash::success(
                Redirect::to(format!("/spars/{}/", spar_id)),
                "Attached that draft draw to this spar!",
            )))
        })
        .unwrap()
    })
    .await
}
