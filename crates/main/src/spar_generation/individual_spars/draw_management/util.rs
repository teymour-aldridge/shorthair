use std::collections::HashMap;

use db::{
    ballot::{AdjudicatorBallot, BallotRepr},
    room::SparRoomRepr,
    schema::adjudicator_ballots,
};
use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite, Connection};
use maud::Markup;

// todo: make method of BallotRepr (?)
//
// or at least place in the same module
pub fn ballots_of_rooms(
    rooms: &[SparRoomRepr],
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
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
pub fn render_draw(
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
                @for (i, room) in room_info.iter().enumerate() {
                    tr {
                        td { (i) }
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
