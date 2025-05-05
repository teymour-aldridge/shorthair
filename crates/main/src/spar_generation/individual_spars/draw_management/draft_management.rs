//! Management for draft draws.

use std::collections::HashMap;

use db::{
    draft_draw::DraftDraw,
    schema::{draft_draws, spar_series, spar_series_members, spars},
    spar::{Spar, SparSeriesMember},
    user::User,
    DbConn,
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use maud::Markup;

use crate::{
    html::page_of_body,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    resources::GroupRef,
    spar_generation::allocation_problem::solve_allocation::SolverRoom,
};

#[get("/spars/<spar_id>/draws/<draw_id>")]
pub async fn view_draft_draw(
    spar_id: &str,
    draw_id: &str,
    db: DbConn,
    user: User,
    span: TracingSpan,
) -> Option<Markup> {
    let span1 = span.0.clone();
    let spar_id = spar_id.to_string();
    let draw_id = draw_id.to_string();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(spar) => spar,
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

            let other_draws = draft_draws::table
                .filter(draft_draws::spar_id.eq(spar.id))
                .order_by(draft_draws::created_at.desc())
                // todo: don't need to load all the data here
                .load::<DraftDraw>(conn)
                .unwrap();

            Ok(Some(render_draft_management_page(
                &draw,
                draw.data
                    .as_ref()
                    .map(|draw| serde_json::from_str(&draw).unwrap()),
                &other_draws,
                &spar,
                user,
                conn,
            )))
        })
        .unwrap()
    })
    .await
}

/// Refreshes the page every 5 seconds (this allows the page to automatically
/// load the draw when it is ready).
fn refresh_tag() -> Markup {
    use maud::html;

    html! {
        meta http-equiv="refresh" content="5";
    }
}

/// Renders the draw data as a table.
#[tracing::instrument(skip(conn))]
pub fn render_draw_data(
    draw_data: HashMap<usize, SolverRoom>,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> Markup {
    // todo: compute this upfront
    let mut get_member = |id: i64| {
        spar_series_members::table
            .filter(spar_series_members::id.eq(id))
            .first::<SparSeriesMember>(conn)
            .unwrap()
    };

    use maud::html;

    html! {
        div class="table-responsive" {
            table class="table table-striped table-bordered" {
                thead class="table-dark" {
                    tr {
                        th { "Room" }
                        th { "Opening Government" }
                        th { "Opening Opposition" }
                        th { "Closing Government" }
                        th { "Closing Opposition" }
                        th { "Panel" }
                    }
                }
                tbody {
                    @for (room_number, room) in draw_data.iter() {
                        tr {
                            td { (room_number) }

                            // Opening Government
                            td {
                                @let team = room.teams.get(&crate::spar_generation::allocation_problem::solve_allocation::Team::Og).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Opening Opposition
                            td {
                                @let team = room.teams.get(&crate::spar_generation::allocation_problem::solve_allocation::Team::Oo).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Closing Government
                            td {
                                @let team = room.teams.get(&crate::spar_generation::allocation_problem::solve_allocation::Team::Cg).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Closing Opposition
                            td {
                                @let team = room.teams.get(&crate::spar_generation::allocation_problem::solve_allocation::Team::Co).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Panel
                            td {
                                @for member_id in &room.panel {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tracing::instrument(skip(conn))]
fn render_draft_management_page(
    current_draw: &DraftDraw,
    draw_data: Option<HashMap<usize, SolverRoom>>,
    all_draws: &[DraftDraw],
    spar: &Spar,
    user: User,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> Markup {
    use maud::html;

    let rendered_data = if let Some(draw_data) = draw_data {
        render_draw_data(draw_data, conn)
    } else {
        maud::html! {
            div class="alert alert-info" role="alert" {
                div class="d-flex align-items-center" {
                    div class="spinner-border text-info me-3" role="status" {
                        span class="visually-hidden" { "Loading..." }
                    }
                    div {
                        h5 class="alert-heading" { "Draw Generation in Progress" }
                        p class="mb-0" { "The draw is currently being generated. This page will automatically refresh when the draw is ready." }
                        p class="small text-muted mt-2" { "Please wait a moment..." }
                    }
                }
                (refresh_tag())
            }
        }
    };

    let markup = html! {
        div class="container-fluid" {
            div class="row" {
                div class="col-md-8 mb-4" {
                    div class="card" {
                        div class="card-header" {
                            h2 { "Current Draw" }
                        }
                        div class="card-body" {
                            p { "Draw ID: " (current_draw.public_id) }
                            p class="mt-3" {
                                a href={"/spars/" (spar.public_id) "/draws/" (current_draw.public_id) "/confirm"} class="btn btn-success" {
                                    "Confirm draw"
                                }
                            }
                            (rendered_data)
                        }
                    }
                }

                div class="col-md-4" {
                    div class="card" {
                        div class="card-header" {
                            h3 { "Draw Management" }
                        }
                        div class="card-body" {
                            h4 { "Spar Details" }
                            p { "Spar ID: " (spar.public_id) }
                            p { "Created: " (spar.created_at) }

                            h4 { "All Draws" }
                            ul class="list-group mt-3" {
                                @for draw in all_draws {
                                    li class="list-group-item" {
                                        a href={"/spars/" (spar.public_id) "/draws/" (draw.public_id)} {
                                            "Draw " (draw.public_id)
                                        }
                                        @if draw.id == current_draw.id {
                                            " (current)"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    page_of_body(markup, Some(user))
}
