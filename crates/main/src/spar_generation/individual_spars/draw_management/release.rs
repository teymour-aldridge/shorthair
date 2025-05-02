// todo: mechanism to edit draws

use std::sync::Arc;

use db::{
    ballot::AdjudicatorBallotLink,
    schema::{
        spar_adjudicator_ballot_links, spar_rooms, spar_series,
        spar_series_members, spars,
    },
    spar::{Spar, SparSeriesMember},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use either::Either;
use email::send_mail;
use maud::Markup;
use rocket::response::Redirect;

use crate::{
    html::{error_403, error_404},
    permissions::{has_permission, Permission},
    resources::GroupRef,
};

#[post("/spars/<spar_id>/releasedraw")]
pub async fn do_release_draw(
    spar_id: String,
    user: User,
    db: DbConn,
) -> Either<Markup, Redirect> {
    let db = Arc::new(db);
    db.clone().run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let spar = match spars::table
                .filter(spars::public_id.eq(&spar_id))
                .first::<Spar>(conn)
                .optional()
                .unwrap()
            {
                Some(spar) => spar,
                None => {
                    return Ok(Either::Left(error_404(
                        Some("No such spar!".to_string()),
                        Some(user),
                    )))
                }
            };

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
                return Ok(Either::Left(error_403(
                    Some(
                        "Error: you don't have permission to do that"
                            .to_string(),
                    ),
                    Some(user),
                )));
            }

            let n = diesel::update(spars::table.filter(spars::id.eq(spar.id)))
                .set((spars::release_draw.eq(true), spars::is_open.eq(false)))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            let adjudicators = spar_adjudicator_ballot_links::table
                .inner_join(spar_rooms::table)
                .filter(spar_rooms::spar_id.eq(spar.id))
                .inner_join(spar_series_members::table)
                .select((
                    spar_adjudicator_ballot_links::all_columns,
                    spar_series_members::all_columns,
                ))
                .load::<(AdjudicatorBallotLink, SparSeriesMember)>(conn)?;

            for (adj_link, member) in adjudicators {
                let ballot_link = format!("https://eldemite.net/ballots/submit/{}", adj_link.link);
                send_mail(
                    vec![(&member.name, &member.email)],
                    "Ballot link",
                    &maud::html! {
                        "Please use " a href=(ballot_link) { "this link" } " to submit your ballot."
                    }.into_string(),
                    &format!("Please use this link to submit your ballot: {ballot_link}"),
                    db.clone()
                );
            }

            Ok(Either::Right(Redirect::to(format!("/spars/{spar_id}"))))
        })
        .unwrap()
    })
    .await
}
