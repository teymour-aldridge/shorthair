use std::{collections::HashMap, sync::Arc};

use chrono::{TimeDelta, Utc};
use db::{
    schema::{
        group_members, groups, spar_adjudicator_ballot_links,
        spar_adjudicators, spar_rooms, spar_series, spar_series_members,
        spar_signups, spar_speakers, spar_teams, spars,
    },
    spar::{Spar, SparSeriesMember, SparSignup},
    user::User,
    DbConn,
};
use diesel::dsl::{exists, insert_into, select};
use diesel::prelude::*;
use rocket::response::{status::Unauthorized, Flash, Redirect};
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    request_ids::TracingSpan,
    spar_generation::allocation_problem::solve_allocation::{
        rooms_of_speaker_assignments, solve_lp, Team,
    },
};

#[post("/spars/<session_id>/makedraw")]
/// Generate the draw for the internal sessions.
///
/// TODO: fix the concurrency behaviour of this code (e.g. might want a
/// ticketing system, so that users can override long-running in-progress
/// draw generations if they would like to)
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
    let span1 = span.0.clone();
    db.clone().run(move |conn| {
        let _guard = span1.enter();
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

            tracing::trace!("Spar exists");

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

            tracing::trace!("User has permission");

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
                        Redirect::to(format!("/spars/{}", spar.public_id)),
                        // todo: format numbers
                        "Error: too few people willing to judge for a British
                        Parliamentary session (assuming 1 judge and 8 people)!",
                    ))));
                }
            };

            tracing::trace!("Basic checks to ensure a valid draw can be
                             generated were met");

            // todo: run this outside of the transaction
            let rooms = {
                let span = tracing::info_span!("draw generation process");
                let _guard = span.enter();
                let elo_scores = crate::spar_generation::allocation_problem::ratings::compute_scores(spar.spar_series_id, conn)?;
                let params = solve_lp(signups.clone(), elo_scores);
                rooms_of_speaker_assignments(&params)
            };

            tracing::trace!("Generated room assignments");

            diesel::delete(spar_rooms::table.filter(spar_rooms::spar_id.eq(spar.id))).execute(conn)?;

            let span = tracing::info_span!("Inserting new rooms into database");
            let guard = span.enter();

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

            drop(guard);

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
