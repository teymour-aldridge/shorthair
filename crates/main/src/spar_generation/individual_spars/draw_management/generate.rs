use std::{collections::HashMap, sync::Arc};

use db::{
    draft_draw::{DraftDrawData, DraftDrawRoom},
    schema::{
        draft_draws, spar_series, spar_series_members, spar_signups, spars,
    },
    spar::{Spar, SparSignup},
    user::User,
    DbConn,
};
use diesel::dsl::insert_into;
use diesel::prelude::*;
use itertools::Itertools;
use rocket::{
    response::{status::Unauthorized, Flash, Redirect},
    tokio,
};
use tracing::Instrument;

use crate::{
    model::sync::id::gen_uuid,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    resources::GroupRef,
    spar_generation::allocation_problem::{
        ratings::compute_scores,
        solve_allocation::{
            rooms_of_speaker_assignments, solve_lp, SolverRoom,
        },
    },
};

#[post("/spars/<session_id>/makedraw")]
/// Generate the draw for the internal sessions.
pub async fn generate_draw(
    user: User,
    session_id: &str,
    db: DbConn,
    span: TracingSpan,
) -> Option<Result<Flash<Redirect>, Unauthorized<()>>> {
    let session_id = session_id.to_string();
    let session_id1 = session_id.clone();
    let db = Arc::new(db);
    let span1 = span.0.clone();

    let ctx = db.clone().run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<Option<Result<(_, _, _, _), _>>, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(session_id))
                .get_result::<Spar>(conn)
                .optional()
                .unwrap();

            let spar = match spar {
                Some(session) => session,
                None => return Ok(None),
            };

            tracing::trace!("Spar exists");

            let user_has_permission = has_permission(
                Some(&user),
                &Permission::ModifyResourceInGroup(GroupRef({
                    spar_series::table
                        .filter(spar_series::id.eq(spar.spar_series_id))
                        .select(spar_series::group_id)
                        .first::<i64>(conn)
                        .unwrap()
                })),
                conn,
            );

            if !user_has_permission {
                return Ok::<_, diesel::result::Error>(Some(Err(Err(Unauthorized(())))));
            }

            tracing::trace!("User has permission");

            let signups = {
                let (signups1, signups2) = diesel::alias!(spar_signups as signups1, spar_signups as signups2);

                let missing_signups = signups1
                    .filter(signups1.field(spar_signups::spar_id).eq(spar.id))
                    .filter(
                        signups1
                            .field(spar_signups::partner_preference)
                            .is_not_null()
                    )
                    .filter(
                        signups1
                            .field(spar_signups::partner_preference)
                            .assume_not_null()
                            .ne_all(
                                signups2
                                    .filter(
                                        signups2.field(spar_signups::spar_id).eq(spar.id)
                                    )
                                    .inner_join(
                                        spar_series_members::table.on(
                                            signups2
                                                .field(spar_signups::member_id)
                                                .eq(spar_series_members::id)
                                        )
                                    )
                                    .filter(
                                        spar_series_members::id.eq(
                                            signups1
                                                .field(spar_signups::partner_preference)
                                                .assume_not_null()
                                        )
                                    )
                                    .select(spar_series_members::id)
                            )
                    )
                    .load::<SparSignup>(conn)
                    .unwrap();

                if !missing_signups.is_empty() {
                    let missing = missing_signups.iter().map(|signup| {
                        let partner_name = spar_series_members::table
                            .filter(spar_series_members::id.eq(
                                signup.partner_preference.unwrap()
                            ))
                            .select(spar_series_members::name)
                            .first::<String>(conn)
                            .unwrap();

                        let own_name = spar_series_members::table
                            .filter(spar_series_members::id.eq(signup.member_id))
                            .select(spar_series_members::name)
                            .first::<String>(conn)
                            .unwrap();

                        format!(
                            "{own_name} (preferred speaking partner {partner_name} has not signed up)"
                        )
                    }).join(", ");

                    return Ok(Some(Err(Ok(Flash::error(
                        Redirect::to(format!("/spars/{}", spar.public_id)),
                        format!(
                            "Error: some speakers have selected preferred speaking \
                             partners who are not signed up for this spar! \
                             Missing: {missing}"
                        ),
                    )))));
                }

                let signups = spar_signups::table
                    .filter(spar_signups::spar_id.eq(spar.id))
                    .load::<SparSignup>(conn)
                    .unwrap();

                Arc::new(
                    signups.into_iter().map(|signup| {
                        (signup.member_id, signup)
                    }).collect::<HashMap<_, _>>()
                )
            };

            let n_judges = signups.iter().filter(|(_id, signup)| signup.as_judge).count();

            // todo: this section is wrong
            let _check_valid_solution_exists = {
                let n_people_only_willing_to_speak = signups
                    .iter()
                    .filter(|(_id, signup)| signup.as_speaker && !signup.as_judge)
                    .count();

                if n_people_only_willing_to_speak < 4 {
                    return Ok(Some(Err(Ok(Flash::error(
                        Redirect::to(format!("/spars/{}", spar.public_id)),
                        "Error: too few speakers for a British Parliamentary spar (need at least 4)!",
                    )))));
                }

                // check whether in the most extreme case (where all those who are
                // willing to both speak and judge are assigned as judges) we have
                // enough people to form a debate
                if n_judges * 8 < n_people_only_willing_to_speak {
                    return Ok(Some(Err(Ok(Flash::error(
                        Redirect::to(format!("/spars/{}", spar.public_id)),
                        // todo: format numbers
                        "Error: too few people willing to judge for a British
                        Parliamentary session (assuming 1 judge and 8 people)!",
                    )))));
                }
            };

            tracing::trace!("Basic checks to ensure a valid draw can be
                             generated were met");

            let elo_scores = compute_scores(spar.spar_series_id, conn)?;

            let draft_id = gen_uuid().to_string();

            let draft_draw_id = insert_into(draft_draws::table)
                .values((
                    draft_draws::public_id.eq(&draft_id),
                    draft_draws::data.eq(None::<String>),
                    draft_draws::spar_id.eq(spar.id),
                    draft_draws::version.eq(0),
                    draft_draws::created_at.eq(diesel::dsl::now),
                ))
                .returning(draft_draws::id)
                .get_result::<i64>(conn)
                .unwrap();


            Ok(Some(Ok((draft_draw_id, draft_id, elo_scores, signups))))
        }).unwrap()
    }).instrument(span.0.clone()).await;

    let (draft_draw_id, draft_public_id, elo_scores, signups) = match ctx {
        Some(Ok(t)) => t,
        Some(Err(t)) => return Some(t),
        None => return None,
    };

    let span2 = span.0.clone();

    rocket::tokio::task::spawn_blocking(move || {
        let _guard = span2.enter();
        tracing::info_span!("generating draw");
        let rooms = {
            let params = solve_lp(signups.clone(), elo_scores);
            let solver_rooms = rooms_of_speaker_assignments(&params);
            solver_room_to_draft_draw(solver_rooms)
        };

        let insertion_span = tracing::trace_span!("inserting generated draw");
        tokio::task::spawn(
            async move {
                let tx_span = tracing::trace_span!("inserting draw tx");
                db.run(move |conn| {
                    let _guard = tx_span.enter();
                    conn.transaction(
                        |conn| -> Result<_, diesel::result::Error> {
                            let n = diesel::update(
                                draft_draws::table
                                    .filter(draft_draws::id.eq(draft_draw_id)),
                            )
                            .set(draft_draws::data.eq(
                                serde_json::to_string_pretty(&rooms).unwrap(),
                            ))
                            .execute(conn)
                            .unwrap();
                            assert_eq!(n, 1);

                            Ok(())
                        },
                    )
                    .unwrap();
                })
                .await
            }
            .instrument(insertion_span),
        )
    });

    return Some(Ok(Flash::success(
        Redirect::to(format!(
            "/spars/{}/draws/{}",
            session_id1, draft_public_id
        )),
        "Draw generation now in progress!",
    )));
}

pub fn solver_room_to_draft_draw(
    t: HashMap<usize, SolverRoom>,
) -> DraftDrawData {
    let mut data = DraftDrawData {
        rooms: t
            .into_iter()
            .sorted_by_key(|(k, _)| *k)
            .map(|k| DraftDrawRoom {
                panel: k.1.panel,
                teams: k.1.teams,
            })
            .collect(),
        id_map: HashMap::new(),
    };
    data.generate_map();
    data
}
