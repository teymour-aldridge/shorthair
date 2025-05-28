use db::{
    schema::{adjudicator_ballots, spar_rooms, spar_series, spars},
    spar::Spar,
    user::User,
    DbConn,
};
use diesel::prelude::*;

use maud::Markup;
use rocket::response::Redirect;

use crate::{
    html::{error_403, page_of_body},
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    util::tx,
};

#[post("/spars/<spar_id>/mark_complete?<force>")]
pub async fn do_mark_spar_complete(
    spar_id: &str,
    user: User,
    db: DbConn,
    // whether we should over-ride issues (e.g. missing ballots, no spar was
    // actually conducted)
    //
    // todo: should people be able to "un-mark" spars?
    force: bool,
    span: TracingSpan,
) -> Option<Result<Redirect, Markup>> {
    let spar_id = spar_id.to_string();
    tx(span, db, move |conn| {
        let spar = spars::table
            .filter(spars::public_id.eq(spar_id))
            .first::<Spar>(conn)
            .optional()
            .unwrap();
        let spar = match spar {
            Some(spar) => spar,
            None => return None,
        };

        let user_has_permission = has_permission(
            Some(&user),
            &Permission::ModifyResourceInGroup(crate::resources::GroupRef(
                spar_series::table
                    .filter(spar_series::id.eq(spar.spar_series_id))
                    .select(spar_series::group_id)
                    .first::<i64>(conn).unwrap(),
            )),
            conn,
        );

        if !user_has_permission {
            return Some(Err(error_403(
                Some("Error: you do not have permission to do that!"),
                Some(user),
            )));
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

            let rooms_with_ballots = spar_rooms::table
                .filter(spar_rooms::spar_id.eq(spar.id))
                .inner_join(adjudicator_ballots::table)
                .select(spar_rooms::all_columns)
                .count()
                .get_result::<i64>(conn)
                .unwrap();

            let total_rooms = spar_rooms::table
                .filter(spar_rooms::spar_id.eq(spar.id))
                .count()
                .get_result::<i64>(conn)
                .unwrap();

            assert!(
                rooms_with_ballots <= total_rooms,
                "error: rooms_without_ballots={rooms_with_ballots} and
                            total_rooms={total_rooms}"
            );
            assert!(
                rooms_with_ballots >= 0,
                "rooms_without_ballots={rooms_with_ballots}"
            );

            let rooms_without_ballots = total_rooms - rooms_with_ballots;

            if rooms_without_ballots > 0 {
                problems.push(Problem::MissingBallots {
                    count: rooms_without_ballots as usize,
                });
            }

            if total_rooms == 0 {
                problems.push(Problem::NoSparStarted);
            }

            if !problems.is_empty() {
                return Some(Err(page_of_body(
                    maud::html! {
                        h1 { "Warning: problems found" }
                        p {
                            "Some issues were found when marking this spar as complete:"
                        }
                        ul {
                            @for problem in &problems {
                                li {
                                    @match problem {
                                        Problem::MissingBallots { count } => {
                                            "Missing ballots: " (count) " rooms don't have ballots submitted"
                                        }
                                        Problem::NoSparStarted => {
                                            "No draw was generated for this spar"
                                        }
                                    }
                                }
                            }
                        }
                        p {
                            "You can still mark this spar as complete by using the form below, but you may want to address these issues first."
                        }
                        form method="post" action=(format!("/spars/{}/mark_complete?force=true", spar.public_id)) {
                            button class="btn btn-danger" type="submit" {
                                "Mark as complete anyway"
                            }
                        }
                        a href=(format!("/spars/{}", spar.public_id)) class="btn btn-secondary" {
                            "Cancel"
                        }
                    },
                    Some(user),
                )));
            }
        }

        let n = diesel::update(spars::table.filter(spars::id.eq(spar.id)))
            .set((spars::is_open.eq(false), spars::is_complete.eq(true)))
            .execute(conn)
            .unwrap();
        assert_eq!(n, 1);
        Some(Ok(Redirect::to(format!("/spars/{}", spar.public_id))))
    }).await
}
