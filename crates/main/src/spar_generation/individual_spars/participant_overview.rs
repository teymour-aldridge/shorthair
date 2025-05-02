use db::{
    room::SparRoomRepr,
    schema::{spar_rooms, spars},
    spar::Spar,
    user::User,
    DbConn,
};
use diesel::prelude::*;
use maud::{html, Markup};
use tracing::Instrument;

use crate::{html::page_of_body, request_ids::TracingSpan};

use super::draw_management::util::{ballots_of_rooms, render_draw};

#[get("/spars/<spar_id>", rank = 2)]
pub async fn single_spar_overview_for_participants_page(
    user: Option<User>,
    db: DbConn,
    spar_id: &str,
    span: TracingSpan,
) -> Option<Markup> {
    let spar_id = spar_id.to_string();
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.clone();
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
    .instrument(span.0)
    .await
}
