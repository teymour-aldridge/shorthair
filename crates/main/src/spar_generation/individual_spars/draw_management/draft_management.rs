//! Management for draft draws.

use db::{
    draft_draw::{DraftDraw, DraftDrawData, EditAction, Team},
    schema::{draft_draws, spar_series, spar_series_members, spars},
    spar::{Spar, SparSeriesMember},
    user::User,
    DbConn,
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use itertools::Itertools;
use maud::Markup;
use rocket::{
    form::Form,
    response::{Flash, Redirect},
};
use tracing::Instrument;

use crate::{
    html::page_of_body,
    permissions::{has_permission, Permission},
    request_ids::TracingSpan,
    resources::GroupRef,
    util::tx,
};

#[get("/spars/<spar_id>/draws/<draw_id>")]
pub async fn view_draft_draw(
    spar_id: &str,
    draw_id: &str,
    db: DbConn,
    user: User,
    span: TracingSpan,
) -> Option<Markup> {
    let draw_id = draw_id.to_string();
    let spar_id = spar_id.to_string();
    tx(span, db, move |conn| {
        let spar = match spars::table
            .filter(spars::public_id.eq(&spar_id))
            .first::<Spar>(conn)
            .optional()
            .unwrap()
        {
            Some(spar) => spar,
            None => return None,
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
            return None;
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
            None => return None,
        };

        let other_draws_of_same_spar = draft_draws::table
            .filter(draft_draws::spar_id.eq(spar.id))
            .order_by(draft_draws::created_at.desc())
            // todo: don't need to load all the data here
            .load::<DraftDraw>(conn)
            .unwrap();

        let draw_data = draw
            .data
            .as_ref()
            .map(|draw| serde_json::from_str(&draw).unwrap());

        Some(render_draft_management_page(
            &draw,
            draw_data,
            &other_draws_of_same_spar,
            &spar,
            user,
            conn,
        ))
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

pub fn render_draw_data(
    draw_data: &DraftDrawData,
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
                    @for (room_number, room) in draw_data.rooms.iter().enumerate() {
                            tr { td { (room_number) }

                            // Opening Government
                            td {
                                @let team = room.teams.get(&Team::Og).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Opening Opposition
                            td {
                                @let team = room.teams.get(&Team::Oo).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Closing Government
                            td {
                                @let team = room.teams.get(&Team::Cg).unwrap();
                                @for member_id in team {
                                    @let member = get_member(*member_id);
                                    div { (member.name) }
                                }
                            }

                            // Closing Opposition
                            td {
                                @let team = room.teams.get(&Team::Co).unwrap();
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

/// Renders the drag and drop interface to edit the draw.
#[tracing::instrument(skip(draw_data, conn, current_draw, spar))]
pub fn render_drag_n_drop_draw_data(
    draw_data: DraftDrawData,
    spar: &Spar,
    current_draw: &DraftDraw,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> Markup {
    // todo: compute this upfront
    let mut get_member = |id: i64| {
        spar_series_members::table
            .filter(spar_series_members::id.eq(id))
            .first::<SparSeriesMember>(conn)
            .unwrap()
    };

    use maud::{html, PreEscaped};

    html! {
        script src="https://cdn.jsdelivr.net/npm/sortablejs@1.15.6/Sortable.min.js" {}

        div class="table-responsive" {
            form id="speaker-swap-form" method="post" action={"/spars/" (spar.public_id) "/draws/" (current_draw.public_id) "/edit"} {
                input type="hidden" id="swap-query" name="query" value="";

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
                        @for (i, room) in draw_data.rooms.iter().enumerate() {
                            tr {
                                td { (i) }

                                // Opening Government
                                td {
                                    div class="sortable-team" data-room={(i)} data-position="og" {
                                        @let team = room.teams.get(&Team::Og).unwrap();
                                        @for member_id in team.iter().sorted_by_key(|x| *x) {
                                            @let member = get_member(*member_id);
                                            @let public_id = draw_data.id_map.get(member_id).unwrap_or(&0);
                                            div class="sortable-speaker" data-id={(public_id)} data-member-id={(member_id)} {
                                                (member.name)
                                            }
                                        }
                                    }
                                }

                                // Opening Opposition
                                td {
                                    div class="sortable-team" data-room={(i)} data-position="oo" {
                                        @let team = room.teams.get(&Team::Oo).unwrap();
                                        @for member_id in team.iter().sorted_by_key(|x| *x) {
                                            @let member = get_member(*member_id);
                                            @let public_id = draw_data.id_map.get(member_id).unwrap_or(&0);
                                            div class="sortable-speaker" data-id={(public_id)} data-member-id={(member_id)} {
                                                (member.name)
                                            }
                                        }
                                    }
                                }

                                // Closing Government
                                td {
                                    div class="sortable-team" data-room={(i)} data-position="cg" {
                                        @let team = room.teams.get(&Team::Cg).unwrap();
                                        @for member_id in team.iter().sorted_by_key(|x| *x) {
                                            @let member = get_member(*member_id);
                                            @let public_id = draw_data.id_map.get(member_id).unwrap_or(&0);
                                            div class="sortable-speaker" data-id={(public_id)} data-member-id={(member_id)} {
                                                (member.name)
                                            }
                                        }
                                    }
                                }

                                // Closing Opposition
                                td {
                                    div class="sortable-team" data-room={(i)} data-position="co" {
                                        @let team = room.teams.get(&Team::Co).unwrap();
                                        @for member_id in team.iter().sorted_by_key(|x| *x) {
                                            @let member = get_member(*member_id);
                                            @let public_id = draw_data.id_map.get(member_id).unwrap_or(&0);
                                            div class="sortable-speaker" data-id={(public_id)} data-member-id={(member_id)} {
                                                (member.name)
                                            }
                                        }
                                    }
                                }

                                // Panel
                                td {
                                    div class="sortable-team" data-room={(i)} data-position="panel" {
                                        @for member_id in room.panel.iter().sorted_by_key(|x| *x)  {
                                            @let member = get_member(*member_id);
                                            @let public_id = draw_data.id_map.get(member_id).unwrap_or(&0);
                                            div class="sortable-speaker" data-id={(public_id)} data-member-id={(member_id)} {
                                                (member.name)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        style {
            r#"
            .sortable-speaker {
                cursor: move;
                padding: 8px;
                margin: 4px 0;
                background: #f8f9fa;
                border: 1px solid #dee2e6;
                border-radius: 4px;
            }
            .sortable-speaker:hover {
                background: #e9ecef;
            }
            .sortable-speaker.sortable-ghost {
                opacity: 0.5;
                background: #cfe2ff;
            }
            .sortable-speaker.sortable-chosen {
                background: #d1e7dd;
            }
            .sortable-team {
                min-height: 40px;
                padding: 5px;
            }
            .selected {
                background-color: #d4edda !important;
                border: 2px solid #28a745;
            }
            "#
        }

        (PreEscaped(r#"
        <script>
            document.addEventListener('DOMContentLoaded', function() {
                const teams = document.querySelectorAll('.sortable-team');
                let selectedSpeakers = [];

                // Initialize Sortable for each team
                teams.forEach(team => {
                    new Sortable(team, {
                        group: 'speakers',
                        animation: 150,
                        ghostClass: 'sortable-ghost',
                        chosenClass: 'sortable-chosen',
                        onEnd: function(evt) {
                            if (evt.from !== evt.to || evt.oldIndex !== evt.newIndex) {
                                const sourceElement = evt.item;
                                const sourceId = sourceElement.dataset.id;

                                // Get the next sibling in the new position or the previous one if at the end
                                let targetElement;
                                if (evt.to.children.length > 1) {
                                    if (evt.newIndex === evt.to.children.length - 1) {
                                        targetElement = evt.to.children[evt.newIndex - 1];
                                    } else {
                                        targetElement = evt.to.children[evt.newIndex + 1];
                                    }

                                    const targetId = targetElement.dataset.id;
                                    const swapQuery = `swap ${sourceId} ${targetId}`;

                                    document.getElementById('swap-query').value = swapQuery;
                                    document.getElementById('speaker-swap-form').submit();
                                } else {
                                    // Single item in list or other edge case
                                    const swapQuery = `swap ${sourceId} ${sourceId}`;
                                    document.getElementById('swap-query').value = swapQuery;
                                    document.getElementById('speaker-swap-form').submit();
                                }
                            }
                        }
                    });
                });

                // Click selection logic
                document.querySelectorAll('.sortable-speaker').forEach(speaker => {
                    speaker.addEventListener('click', function(e) {
                        if (e.ctrlKey || e.metaKey) {
                            e.preventDefault(); // Prevent default selection

                            // Add or remove from selected list
                            if (this.classList.contains('selected')) {
                                this.classList.remove('selected');
                                selectedSpeakers = selectedSpeakers.filter(s => s !== this);
                            } else {
                                // Only allow two selections
                                if (selectedSpeakers.length < 2) {
                                    this.classList.add('selected');
                                    selectedSpeakers.push(this);

                                    // If we now have exactly 2 selections, automatically submit the form
                                    if (selectedSpeakers.length === 2) {
                                        const id1 = selectedSpeakers[0].dataset.id;
                                        const id2 = selectedSpeakers[1].dataset.id;
                                        document.getElementById('swap-query').value = `swap ${id1} ${id2}`;
                                        document.getElementById('speaker-swap-form').submit();
                                    }
                                } else {
                                    alert('You can only select two speakers at a time for swapping');
                                }
                            }
                        }
                    });
                });
            });
        </script>
        "#))
    }
}

#[tracing::instrument(skip(
    current_draw,
    draw_data,
    all_draws,
    spar,
    user,
    conn
))]
fn render_draft_management_page(
    current_draw: &DraftDraw,
    draw_data: Option<DraftDrawData>,
    all_draws: &[DraftDraw],
    spar: &Spar,
    user: User,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> Markup {
    use maud::html;

    let rendered_data = if let Some(draw_data) = draw_data {
        render_drag_n_drop_draw_data(draw_data, spar, current_draw, conn)
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
            // Custom CSS for navigation bar color
            style {
                r#"
                .navbar {
                    background-color: #E32879 !important;
                }
                .navbar .nav-link, .navbar-brand {
                    color: white !important;
                }
                .card-header {
                    background-color: rgba(227, 40, 121, 0.1);
                    border-bottom: 1px solid rgba(227, 40, 121, 0.2);
                }
                .btn-primary {
                    background-color: #E32879;
                    border-color: #E32879;
                }
                .btn-primary:hover {
                    background-color: #c71e66;
                    border-color: #c71e66;
                }
                "#
            }
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
                                        a href={"/spars/" (spar.public_id) "/draws/" (draw.public_id)} style="color: #E32879;" {
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

#[derive(FromForm)]
pub struct EditDrawForm {
    pub query: String,
}

#[post("/spars/<spar_id>/draws/<draw_id>/edit", data = "<form>")]
pub async fn do_edit_draw(
    spar_id: &str,
    draw_id: &str,
    user: User,
    db: DbConn,
    form: Form<EditDrawForm>,
    span: TracingSpan,
) -> Option<Flash<Redirect>> {
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

            let draw = match draft_draws::table
                .filter(draft_draws::public_id.eq(&draw_id))
                .first::<DraftDraw>(conn)
                .optional()
                .unwrap()
            {
                Some(draw) => draw,
                None => return Ok(None),
            };

            // todo: return proper error messages to the user
            let data: DraftDrawData = match draw.data {
                Some(data) => match serde_json::from_str(&data) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!("Encountered unexpected JSON parsing error when trying to load draw data: {e:?}");
                        return Ok(None)
                    },
                },
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
                return Ok(None);
            }

            let action = match EditAction::parse(&form.query) {
                Ok(t) => t,
                Err(e) => {
                    return Ok(Some(Flash::error(
                        Redirect::to(format!(
                            "/spars/{spar_id}/draws/{draw_id}"
                        )),
                        e.to_string(),
                    )))
                }
            };

            let new_data = match data.apply(action) {
                Ok(t) => t,
                Err(e) => {
                    return Ok(Some(Flash::error(
                        Redirect::to(format!(
                            "/spars/{spar_id}/draws/{draw_id}"
                        )),
                        e.to_string(),
                    )))
                },
            };

            let n = diesel::update(draft_draws::table.filter(draft_draws::id.eq(draw.id)))
                .set(draft_draws::data.eq(serde_json::to_string_pretty(&new_data).unwrap()))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            return Ok(Some(Flash::success(
                Redirect::to(format!(
                    "/spars/{spar_id}/draws/{draw_id}"
                )),
                "Applied that action",
            )))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}
