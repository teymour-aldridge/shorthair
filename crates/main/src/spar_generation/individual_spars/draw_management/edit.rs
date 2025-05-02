use db::{
    room::SparRoomRepr,
    schema::{spar_rooms, spar_series, spars},
    spar::Spar,
    user::User,
    DbConn,
};
use diesel::prelude::*;
use maud::{html, Markup};

use crate::{
    html::{error_403, error_404, page_of_body},
    permissions::{has_permission, Permission},
    resources::GroupRef,
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

            let user_is_admin = has_permission(
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
