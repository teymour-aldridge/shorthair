use db::{
    room::SparRoomRepr,
    schema::{group_members, groups, spar_rooms, spar_series, spars},
    spar::Spar,
    user::User,
    DbConn,
};
use diesel::dsl::{exists, select};
use diesel::prelude::*;
use maud::{html, Markup};

use crate::{
    html::{error_403, error_404, page_of_body},
    spar_generation::individual_spars::draw_management::util::render_draw,
};

use super::util::ballots_of_rooms;

#[get("/spars/<spar_id>/showdraw")]
pub async fn show_draw_to_admin_page(
    spar_id: String,
    user: User,
    db: DbConn,
) -> Markup {
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

            let markup = html! {
                @if user_is_admin {
                    form method="post" action="releasedraw" {
                        button class="btn btn-primary" type="submit" {
                            "Release draw"
                        }
                    }
                }

                (render_draw(draw_info, ballots))
            };

            Ok(page_of_body(markup, Some(user)))
        })
        .unwrap()
    })
    .await
}
