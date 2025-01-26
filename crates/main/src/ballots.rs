use std::sync::Arc;

use db::{
    schema::{
        adjudicator_ballot_submissions, spar_room_adjudicator, spar_rooms,
        spars,
    },
    spar::{AdjudicatorBallotSubmission, SparRoomAdjudicator},
    user::User,
    DbConn,
};
use diesel::prelude::*;
use email::send_mail;
use maud::Markup;
use rocket::{form::Form, tokio};
use serde::{Deserialize, Serialize};

use crate::html::error_403;
use crate::html::page_of_body;

#[derive(Serialize, Deserialize)]
pub struct Ballot {
    og: BallotTeam,
    oo: BallotTeam,
    cg: BallotTeam,
    co: BallotTeam,
}

#[derive(Serialize, Deserialize)]
pub struct BallotTeam {
    s1: u16,
    s2: u16,
}

#[get("/sessions/<session_id>/makeballot")]
/// Allows a judge to submit their ballot.
///
/// Requires the user to be
/// (1) signed in
/// (2) set up as a judge
/// (3) draw must have been released
pub async fn submit_ballot_page(
    session_id: String,
    user: User,
    db: DbConn,
) -> Markup {
    db.run(move |conn| {
        let adjudicator: Option<SparRoomAdjudicator> = spar_room_adjudicator::table
            .filter(spar_room_adjudicator::user_id.eq(user.id))
            .inner_join(spar_rooms::table.inner_join(spars::table))
            .filter(spars::public_id.eq(&session_id))
            .select(spar_room_adjudicator::all_columns)
            .first::<SparRoomAdjudicator>(conn)
            .optional()
            .unwrap();

        let adjudicator: SparRoomAdjudicator = match adjudicator {
            Some(adj) => adj,
            None => {
                return error_403(
                    Some("You are not allocated as an adjudicator for this spar, so cannot submit a ballot.".to_string()),
                    Some(user),
                );
            }
        };

        let latest_ballot = adjudicator_ballot_submissions::table
            .order(adjudicator_ballot_submissions::created_at.desc())
            .select(adjudicator_ballot_submissions::created_at)
            .first::<chrono::NaiveDateTime>(conn)
            .optional()
            .unwrap();

        if let Some(latest_ballot) = latest_ballot {
            if (chrono::Utc::now().naive_utc() - latest_ballot).num_hours() > 1 {
                return error_403(
                    Some("Ballot submissions for this session are now closed.".to_string()),
                    Some(user),
                );
            }
        }

        let ballot = adjudicator_ballot_submissions::table
            .filter(adjudicator_ballot_submissions::room_id.eq(adjudicator.room_id))
            .filter(adjudicator_ballot_submissions::adjudicator_id.eq(adjudicator.id))
            .first::<AdjudicatorBallotSubmission>(conn)
            .optional()
            .unwrap();
        let ballot: Option<Ballot> =
            ballot.map(|adj_ballot| serde_json::from_str(&adj_ballot.ballot_data).unwrap());

        submit_ballot_form(ballot, None, user)
    })
    .await
}

fn submit_ballot_form(
    ballot: Option<Ballot>,
    prev_input: Option<Ballot>,
    user: User,
) -> Markup {
    page_of_body(
        maud::html! {
            h1 { "Submit ballot" }

            @if let Some(existing) = ballot {
                p { "Note: you have already submitted the following ballot." }

                div.container {
                    div.row {
                        div.col {
                            h4 { "Opening Government" }
                            p { "Prime Minister: " (existing.og.s1) }
                            p { "Deputy Prime Minister: " (existing.og.s2) }
                            p { "Total: " (existing.og.s1 + existing.og.s2) }
                        }
                        div.col {
                            h4 { "Opening Opposition" }
                            p { "Leader of Opposition: " (existing.oo.s1) }
                            p { "Deputy Leader of Opposition: " (existing.oo.s2) }
                            p { "Total: " (existing.oo.s1 + existing.oo.s2) }
                        }
                    }
                    div.row {
                        div.col {
                            h4 { "Closing Government" }
                            p { "Member of Government: " (existing.cg.s1) }
                            p { "Government Whip: " (existing.cg.s2) }
                            p { "Total: " (existing.cg.s1 + existing.cg.s2) }
                        }
                        div.col {
                            h4 { "Closing Opposition" }
                            p { "Member of Opposition: " (existing.co.s1) }
                            p { "Opposition Whip: " (existing.co.s2) }
                            p { "Total: " (existing.co.s1 + existing.co.s2) }
                        }
                    }
                }

                b { "If your ballot is outdated, you can submit a new one. This will override the previous ballot. Other judges will receive an email notification informing them that you have submitted this ballot." }
            } @else {
                p { "Submit a ballot for this debate" }
            }

            script { "
            function recomputeAll() {
              let pm = document.getElementById('pm').valueOf;
              let dpm = document.getElementById('dpm').valueOf;
              let ogTotal = pm + dpm;
              let lo = document.getElementById('lo').valueOf;
              let dlo = document.getElementById('dlo').valueOf;
              let ooTotal = lo + dlo;
              let mg = document.getElementById('mg').valueOf;
              let gw = document.getElementById('gw').valueOf;
              let cgTotal = mg + gw;
              let mo = document.getElementById('mo').valueOf;
              let ow = document.getElementById('ow').valueOf;
              let coTotal = mo + ow;

              if (ogTotal === ooTotal || ogTotal === cgTotal || ogTotal === coTotal ||
                  ooTotal === cgTotal || ooTotal === coTotal ||
                  cgTotal === coTotal) {
                    document.getElementById('error').innerHTML =
                      'Error: teams cannot have equal scores';
              }

              document.getElementById('og-total').innerHTML = ogTotal;
              document.getElementById('oo-total').innerHTML = ooTotal;
              document.getElementById('cg-total').innerHTML = cgTotal;
              document.getElementById('co-total').innerHTML = coTotal;
            }
        " }

            div.container {
                form {
                    div.alert.alert-warning role="alert" id="error" { }
                    div.row {
                        div.col {
                            input name="pm" id="pm" placeholder="Prime minister's speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.og.s1.to_string()).unwrap_or("".to_string())) {}
                            input name="dpm" id="dpm" placeholder="Deputy prime minister's speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.og.s2.to_string()).unwrap_or("".to_string())) {}
                            span id="og-total" {}
                        }
                        div.col {
                            input name="lo" id="lo" placeholder="Leader of opposition speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.oo.s1.to_string()).unwrap_or("".to_string())) {}
                            input name="dlo" id="dlo" placeholder="Deputy leader of the opposition speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.oo.s2.to_string()).unwrap_or("".to_string())) {}
                            span id="oo-total" {}
                        }
                    }
                    div.row {
                        div.col {
                            input name="mg" id="mg" placeholder="Member of government speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.cg.s1.to_string()).unwrap_or("".to_string())) {}
                            input name="gw" id="gw" placeholder="Government whip speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.cg.s2.to_string()).unwrap_or("".to_string())) {}
                            span id="cg-total" {}
                        }
                        div.col {
                            input name="mo" id="mo" placeholder="Member of opposition speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.co.s1.to_string()).unwrap_or("".to_string())) {}
                            input name="ow" id="ow" placeholder="Opposition whip speaks" type="number" max="100" min="50" onchange="recomputeAll()" value=(prev_input.as_ref().map(|b| b.co.s2.to_string()).unwrap_or("".to_string())) {}
                            span id="co-total" {}
                        }
                    }
                    button type="submit" { "Submit ballot" }
                }
            }
        },
        Some(user),
    )
}

#[derive(FromForm)]
pub struct BallotForm {
    pm: u16,
    dpm: u16,
    lo: u16,
    dlo: u16,
    mg: u16,
    gw: u16,
    mo: u16,
    ow: u16,
}

#[post("/sessions/<session_id>/makeballot", data = "<form>")]
pub async fn do_submit_ballot(
    session_id: String,
    user: User,
    form: Form<BallotForm>,
    db: DbConn,
) -> Markup {
    let (template, emails) = db.run(move |conn| {
        let emails = Vec::with_capacity(10);

        let adjudicator: Option<SparRoomAdjudicator> = spar_room_adjudicator::table
            .filter(spar_room_adjudicator::user_id.eq(user.id))
            .inner_join(spar_rooms::table.inner_join(spars::table))
            .filter(spars::public_id.eq(&session_id))
            .select(spar_room_adjudicator::all_columns)
            .first::<SparRoomAdjudicator>(conn)
            .optional()
            .unwrap();

        let adjudicator: SparRoomAdjudicator = match adjudicator {
            Some(adj) => adj,
            None => {
                return (error_403(
                    Some("You are not allocated as an adjudicator for this spar, so cannot submit a ballot.".to_string()),
                    Some(user),
                ), emails);
            }
        };

        let ballot = adjudicator_ballot_submissions::table
            .filter(adjudicator_ballot_submissions::room_id.eq(adjudicator.room_id))
            .filter(adjudicator_ballot_submissions::adjudicator_id.eq(adjudicator.id))
            .first::<AdjudicatorBallotSubmission>(conn)
            .optional()
            .unwrap();

        let time_of_previous_ballot = adjudicator_ballot_submissions::table
            .order(adjudicator_ballot_submissions::created_at.desc())
            .select(adjudicator_ballot_submissions::created_at)
            .first::<chrono::NaiveDateTime>(conn)
            .optional()
            .unwrap();

        if let Some(latest_ballot) = time_of_previous_ballot {
            if (chrono::Utc::now().naive_utc() - latest_ballot).num_hours() > 1 {
                return (error_403(
                    Some("Ballot submissions for this session are now closed.".to_string()),
                    Some(user),
                ), emails);
            }
        }

        let ballot: Option<Ballot> =
            ballot.map(|adj_ballot| serde_json::from_str(&adj_ballot.ballot_data).unwrap());

        let (is_valid, _) = validity_of_ballot(&form);

        let insertable_ballot = Ballot {
            og: BallotTeam {
                s1: form.pm,
                s2: form.dpm,
            },
            oo: BallotTeam {
                s1: form.lo,
                s2: form.dlo,
            },
            cg: BallotTeam {
                s1: form.mg,
                s2: form.gw,
            },
            co: BallotTeam {
                s1: form.mo,
                s2: form.ow,
            },
        };

        if !is_valid {
            return (
                submit_ballot_form(ballot, Some(insertable_ballot), user),
                emails
            );
        }

        let ballot_json = serde_json::to_string(&insertable_ballot).unwrap();

        diesel::insert_into(adjudicator_ballot_submissions::table)
            .values((
                adjudicator_ballot_submissions::room_id.eq(adjudicator.room_id),
                adjudicator_ballot_submissions::adjudicator_id.eq(adjudicator.id),
                adjudicator_ballot_submissions::ballot_data.eq(ballot_json),
                adjudicator_ballot_submissions::created_at.eq(diesel::dsl::now),
            ))
            .execute(conn)
            .unwrap();

        (submit_ballot_form(ballot, None, user), emails)
    })
    .await;

    // todo: send_emailS function to send multiple emails
    tokio::spawn(async move {
        let db = Arc::new(db);
        for each in emails {
            let (recipients, subject, html_content, text_content): (
                Vec<(String, String)>,
                String,
                String,
                String,
            ) = each;
            send_mail(
                recipients
                    .iter()
                    .map(|(x, y)| (x.as_str(), y.as_str()))
                    .collect::<Vec<_>>(),
                &subject,
                &html_content,
                &text_content,
                db.clone(),
            )
            .await;
        }
    });

    template
}

/// Checks if a ballot is valid.
pub fn validity_of_ballot(ballot: &BallotForm) -> (bool, Option<String>) {
    let og = ballot.pm + ballot.dpm;
    let oo = ballot.lo + ballot.dlo;
    let cg = ballot.mg + ballot.gw;
    let co = ballot.mo + ballot.ow;

    if og == oo {
        return (
            false,
            Some(
                "Opening Government and Opening Opposition have the same score"
                    .to_string(),
            ),
        );
    }
    if og == cg {
        return (
            false,
            Some(
                "Opening Government and Closing Government have the same score"
                    .to_string(),
            ),
        );
    }
    if og == co {
        return (
            false,
            Some(
                "Opening Government and Closing Opposition have the same score"
                    .to_string(),
            ),
        );
    }
    if oo == cg {
        return (
            false,
            Some(
                "Opening Opposition and Closing Government have the same score"
                    .to_string(),
            ),
        );
    }
    if oo == co {
        return (
            false,
            Some(
                "Opening Opposition and Closing Opposition have the same score"
                    .to_string(),
            ),
        );
    }
    if cg == co {
        return (
            false,
            Some(
                "Closing Government and Closing Opposition have the same score"
                    .to_string(),
            ),
        );
    }

    (true, None)
}
