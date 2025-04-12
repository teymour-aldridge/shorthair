use db::schema::{
    adjudicator_ballots, spar_adjudicators, spar_rooms, spar_series,
    spar_series_members,
};
use db::spar::SparSeries;
use db::{schema::spars, spar::Spar, user::User, DbConn};
use diesel::prelude::*;
use diesel::Connection;
use maud::Markup;

use crate::html::{error_404, page_of_body};
use crate::spar_generation::ballots::render_ballot;

#[get("/spar_series/<series_id>/results")]
/// Displays the results of a spar series.
///
/// Currently this displays a list of spars where at least one room has a
/// ballot.
pub async fn results_of_spar_series_page(
    series_id: String,
    user: Option<User>,
    db: DbConn,
) -> Markup {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let series = {
                let t = spar_series::table
                    .filter(spar_series::public_id.eq(series_id))
                    .first::<SparSeries>(conn)
                    .optional()?;
                match t {
                    None => {
                        return Ok(error_404(
                            Some("no such spar".to_string()),
                            user,
                        ))
                    }
                    Some(series) => series,
                }
            };

            let spars = spars::table
                .filter(spars::spar_series_id.eq(series.id))
                .inner_join(
                    spar_rooms::table.inner_join(adjudicator_ballots::table),
                )
                .order_by(spars::created_at.desc())
                .select(spars::all_columns)
                .load::<Spar>(conn)?;

            let markup = maud::html! {
                table class="table" {
                    thead {
                        tr {
                            th scope="col" {
                                "#"
                            }
                            th scope="col" {
                                "Spar time"
                            }
                            th scope="col" {
                                "View results"
                            }
                        }
                    }
                    tbody {
                        @for (i, spar) in spars.iter().enumerate() {
                            tr {
                                th scope="row" {
                                    (i)
                                }
                                td {
                                    (spar.start_time)
                                }
                                td {
                                    a href=(format!("/spar/{}/results", spar.public_id)) {
                                        "View results"
                                    }
                                }
                            }
                        }
                    }
                }

            };

            Ok(
                page_of_body(markup, user)
            )
        })
        .unwrap()
    })
    .await
}

#[get("/spar/<spar_id>/results")]
/// Displays the results of a single spar. Currently we resolve conflicting
/// ballots by assuming that the most recent one is correct.
pub async fn results_of_spar_page(
    spar_id: String,
    user: Option<User>,
    db: DbConn,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()?;

            match spar {
                None => return Ok(None),
                Some(spar) => {
                    let canonical_ballots = spar.canonical_ballots(conn)?;
                    let rooms = spar.rooms(conn)?;
                    let mut rendered_ballots =
                        Vec::with_capacity(canonical_ballots.len());

                    for room in rooms {
                        let assoc_ballot =
                            canonical_ballots.iter().find(|ballot| {
                                ballot.inner.room_id == room.inner.id
                            });
                        if let Some(ballot) = assoc_ballot {
                            let adj_name = spar_series_members::table
                                .inner_join(spar_adjudicators::table)
                                .filter(
                                    spar_adjudicators::id
                                        .eq(ballot.inner.adjudicator_id),
                                )
                                .select(spar_series_members::name)
                                .first::<String>(conn)?;
                            rendered_ballots.push(maud::html! {
                                // todo: come up with better room numbering
                                // system
                                h3 {"Results for room " (room.inner.public_id)}
                                p {
                                    b {
                                        "Submitted by "
                                    }
                                    (adj_name)
                                }
                                p {
                                    "Note: this is the most recent ballot,
                                        which was submitted at "
                                        (ballot.inner.created_at.to_string())
                                    " If this data is wrong, please ask any of
                                    the adjudicators on the panel to submit a
                                    new ballot with the correct data."
                                }
                                (render_ballot(&room, &ballot))
                            });
                        } else {
                            rendered_ballots.push(maud::html! {
                                h3 {"Missing ballot for this room."}
                            })
                        }
                    }

                    let markup = maud::html! {
                        h1 {"Results for spar"}

                        @for ballot in rendered_ballots {
                            (ballot)
                        }
                    };

                    Ok(Some(page_of_body(markup, user)))
                }
            }
        })
        .unwrap()
    })
    .await
}
