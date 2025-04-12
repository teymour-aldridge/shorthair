use arbitrary::Arbitrary;
use db::{
    ballot::{
        AdjudicatorBallot, AdjudicatorBallotLink, BallotRepr, Scoresheet,
        SpeakerScoresheet, TeamScoresheet,
    },
    room::SparRoomRepr,
    schema::{
        adjudicator_ballot_entries, adjudicator_ballots,
        spar_adjudicator_ballot_links, spar_adjudicators, spar_series_members,
        spar_speakers,
    },
    user::User,
    DbConn,
};
use diesel::prelude::*;
use fuzzcheck::DefaultMutator;
use maud::Markup;
use rocket::{form::Form, response::Redirect};
use serde::{Deserialize, Serialize};

use crate::{
    html::{error_404, page_of_body},
    model::sync::id::gen_uuid,
};

#[get("/ballots/submit/<key>")]
pub async fn submit_ballot_page(
    key: String,
    db: DbConn,
    user: Option<User>,
) -> Markup {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let key = match spar_adjudicator_ballot_links::table
                .filter(spar_adjudicator_ballot_links::link.eq(&key))
                .filter(
                    spar_adjudicator_ballot_links::expires_at
                        .gt(diesel::dsl::now),
                )
                .first::<AdjudicatorBallotLink>(conn)
                .optional()?
            {
                Some(key) => key,
                None => {
                    return Ok(error_404(
                        Some(
                            "Error: no such link (perhaps it has expired?)"
                                .to_string(),
                        ),
                        user,
                    ))
                }
            };

            let room = SparRoomRepr::of_id(key.room_id, conn)?;

            let previous_ballot_id = adjudicator_ballots::table
                .filter(adjudicator_ballots::room_id.eq(room.inner.id))
                .inner_join(spar_adjudicators::table)
                .filter(spar_adjudicators::member_id.eq(key.member_id))
                .select(adjudicator_ballots::id)
                .first::<i64>(conn)
                .optional()?;

            let previous_ballot =
                if let Some(previous_ballot_id) = previous_ballot_id {
                    Some(BallotRepr::of_id(previous_ballot_id, conn)?)
                } else {
                    None
                };

            Ok(render_ballot_form(previous_ballot, room, None, user, false))
        })
        .unwrap()
    })
    .await
}

pub fn render_ballot(room: &SparRoomRepr, prev: &BallotRepr) -> Markup {
    maud::html! {
        div class="row pl-3 pt-3 p-0" {
            div class="col-6 list-group mb-3" {
                li class="list-group-item" {
                    strong {"PM "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[0].speakers[0].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[0].speakers[0].score)
                    }
                }
                li class="list-group-item" {
                    strong {"DPM "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[0].speakers[1].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[0].speakers[1].score)
                    }
                }
                li class="list-group-item list-group-item-danger" {
                    em {"Total for Opening Government"}
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[0].speakers[0].score + prev.scoresheet.teams[0].speakers[1].score)
                    }
                }
            }

            div class="col-6 list-group mb-3" {
                li class="list-group-item" {
                    strong {"LO "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[1].speakers[0].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[1].speakers[0].score)
                    }
                }
                li class="list-group-item" {
                    strong {"DLO "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[1].speakers[1].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[1].speakers[1].score)
                    }
                }
                li class="list-group-item list-group-item-info" {
                    em {"Total for Opening Opposition"}
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[1].speakers[0].score + prev.scoresheet.teams[1].speakers[1].score)
                    }
                }
            }

            div class="col-6 list-group mb-3" {
                li class="list-group-item" {
                    strong {"MG "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[2].speakers[0].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[2].speakers[0].score)
                    }
                }
                li class="list-group-item" {
                    strong {"GW "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[2].speakers[1].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[2].speakers[1].score)
                    }
                }
                li class="list-group-item list-group-item-warning" {
                    em {"Total for Closing Government"}
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[2].speakers[0].score + prev.scoresheet.teams[2].speakers[1].score)
                    }
                }
            }

            div class="col-6 list-group mb-3" {
                li class="list-group-item" {
                    strong {"MO "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[3].speakers[0].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[3].speakers[0].score)
                    }
                }
                li class="list-group-item" {
                    strong {"OW "}
                    (room.members[&room.speakers[&prev.scoresheet.teams[3].speakers[1].speaker_id].member_id].name)
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[3].speakers[1].score)
                    }
                }
                li class="list-group-item list-group-item-success" {
                    em {"Total for Closing Opposition"}
                    span class="float-end badge text-bg-secondary" {
                        (prev.scoresheet.teams[3].speakers[0].score + prev.scoresheet.teams[3].speakers[1].score)
                    }
                }
            }
        }
    }
}

/// Renders the ballot submission form.
///
/// The `force_submit` variable should be set to allow the user to submit a
/// ballot that is different to the previously submitted ballot.
///
/// TODO: embed Javascript to automatically display scores on the webpage and
/// warn if the ballot is invalid
fn render_ballot_form(
    prev: Option<BallotRepr>,
    room: SparRoomRepr,
    error: Option<&str>,
    user: Option<User>,
    force_submit: bool,
) -> Markup {
    let prev = prev.map(|prev| {
        maud::html! {
            div class="alert alert-danger" role="alert" {
                p { b { "You have already submitted the following ballot:" } }
                (render_ballot(&room, &prev))
            }
        }
    });

    let teams = room.teams.iter().enumerate().map(|(i, team)| {
        let speaker_names_and_public_ids = team.speakers.iter().map(|speaker_id| {
            let speaker_record = &room.speakers[speaker_id];
            assert_eq!(speaker_record.id, *speaker_id);
            let member = &room.members[&speaker_record.member_id];
            (member.name.clone(), speaker_record.public_id.clone())
        }).collect::<Vec<_>>();

        let (s1, s2) = match i {
            0 => ("pm", "dpm"),
            1 => ("lo", "dlo"),
            2 => ("mg", "gw"),
            3 => ("mo", "ow"),
            _ => unreachable!(),
        };

        maud::html! {
            p {b {(s1)}}
            div class="mb-3" {
                label for=(s1) class="form-label" {"Select Speaker"}
                select name=(s1) id=(s1) class="form-select mb-3" {
                    @for (speaker_name, speaker_id) in &speaker_names_and_public_ids {
                        option value=(speaker_id) {(speaker_name)}
                    }
                }
            }
            div class="mb-3" {
                label for=(s1.to_string() + "_score") class="form-label" {"Speaker Score"}
                input type="number" min="50" max="100" name=(s1.to_string() + "_score") id=(s1.to_string() + "_score") class="form-control" {}
            }
            hr {}
            p {b {(s2)}}
            div class="mb-3" {
                label for=(s2) class="form-label" {"Select Speaker"}
                select name=(s2) id=(s2) class="form-select mb-3" {
                    @for (speaker_name, speaker_id) in &speaker_names_and_public_ids {
                        option value=(speaker_id) {(speaker_name)}
                    }
                }
            }
            div class="mb-3" {
                label for=(s2.to_string() + "_score") class="form-label" {"Speaker Score"}
                input type="number" min="50" max="100" name=(s2.to_string() + "_score") id=(s2.to_string() + "_score") class="form-control" {}
            }
        }
    }).collect::<Vec<_>>();

    let markup = maud::html! {
        h1 {"Ballot submission"}
        @if let Some(error) = error {
            div class="alert alert-danger" role="alert" {
                p {(error)}
            }
        }
        @if let Some(prev) = prev {
            (prev)
        }
        form method="post" {
            div class="row" {
                div class="col" {
                    (teams[0])
                }
                div class="col" {
                    (teams[1])
                }
            }
            div class="row" {
                div class="col" {
                    (teams[2])
                }
                div class="col" {
                    (teams[3])
                }
            }
            @if force_submit {
                input type="checkbox" name="force" hidden checked {}
            } else {
                input type="checkbox" name="force" hidden {}
            }
            button type="submit" class="btn btn-primary" {"Submit"}
        }
    };

    page_of_body(markup, user)
}

#[derive(
    FromForm, Arbitrary, Debug, DefaultMutator, Clone, Serialize, Deserialize,
)]
pub struct BpBallotForm {
    pub pm: String,
    pub pm_score: i64,
    pub dpm: String,
    pub dpm_score: i64,
    pub lo: String,
    pub lo_score: i64,
    pub dlo: String,
    pub dlo_score: i64,
    pub mg: String,
    pub mg_score: i64,
    pub gw: String,
    pub gw_score: i64,
    pub mo: String,
    pub mo_score: i64,
    pub ow: String,
    pub ow_score: i64,
    pub force: bool,
}

#[post("/ballots/submit/<key>", data = "<ballot>")]
pub async fn do_submit_ballot(
    key: String,
    db: DbConn,
    user: Option<User>,
    ballot: Form<BpBallotForm>,
) -> Result<Redirect, Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let key = match spar_adjudicator_ballot_links::table
                .filter(spar_adjudicator_ballot_links::link.eq(&key))
                .filter(
                    spar_adjudicator_ballot_links::expires_at
                        .gt(diesel::dsl::now),
                )
                .first::<AdjudicatorBallotLink>(conn)
                .optional()?
            {
                Some(key) => key,
                None => {
                    return Ok(Err(error_404(
                        Some(
                            "Error: no such link (perhaps it has expired?)"
                                .to_string(),
                        ),
                        user,
                    )))
                }
            };

            let id_of_speaker_uuid =
                |uid: &str, conn: &mut SqliteConnection| {
                    spar_speakers::table
                        .filter(spar_speakers::public_id.eq(uid))
                        .select(spar_speakers::id)
                        .first::<i64>(conn)
                };

            let room = SparRoomRepr::of_id(key.room_id, conn)?;

            let ballot_error = (|| -> Result<_, diesel::result::Error> {
                // check that all speakers are valid

                // todo: warn if unexpected ironman
                let og = &room.teams[0].speakers;
                let pm_i64 = id_of_speaker_uuid(&ballot.pm, conn)?;
                let dpm_i64 = id_of_speaker_uuid(&ballot.dpm, conn)?;
                if !(og.contains(&pm_i64) && og.contains(&dpm_i64)) {
                    return Ok(Some(
                        "Error: the ballot submitted specifies a speaker who is
                        not assigned to this spar (either PM or DPM is
                        incorrect).",
                    ));
                }

                let oo = &room.teams[1].speakers;
                let lo_i64 = id_of_speaker_uuid(&ballot.lo, conn)?;
                let dlo_i64 = id_of_speaker_uuid(&ballot.dlo, conn)?;
                if !(oo.contains(&lo_i64) && oo.contains(&dlo_i64)) {
                    return Ok(Some(
                        "Error: the ballot submitted specifies a speaker who is
                        not assigned to this spar (either LO or DLO is
                        incorrect).",
                    ));
                }

                let cg = &room.teams[2].speakers;
                let mg_i64 = id_of_speaker_uuid(&ballot.mg, conn)?;
                let gw_i64 = id_of_speaker_uuid(&ballot.gw, conn)?;
                if !(cg.contains(&mg_i64) && cg.contains(&gw_i64)) {
                    return Ok(Some(
                        "Error: the ballot submitted specifies a speaker who is
                        not assigned to this spar (either MG or GW is
                        incorrect).",
                    ));
                }

                let co = &room.teams[3].speakers;
                let mo_i64 = id_of_speaker_uuid(&ballot.mo, conn)?;
                let ow_i64 = id_of_speaker_uuid(&ballot.ow, conn)?;
                if !(co.contains(&mo_i64) && co.contains(&ow_i64)) {
                    return Ok(Some(
                        "Error: the ballot submitted specifies a speaker who is
                        not assigned to this spar (either MO or OW is
                        incorrect).",
                    ));
                }

                let og_score = ballot.pm_score + ballot.dpm_score;
                let oo_score = ballot.lo_score + ballot.dlo_score;
                let cg_score = ballot.mg_score + ballot.gw_score;
                let co_score = ballot.mo_score + ballot.ow_score;

                if og_score == oo_score {
                    return Ok(Some(
                        "Error: OG and OO have the same sum of speaks.",
                    ));
                }

                if og_score == cg_score {
                    return Ok(Some(
                        "Error: OG and CG have the same sum of speaks.",
                    ));
                }

                if og_score == co_score {
                    return Ok(Some(
                        "Error: OG and CO have the same sum of speaks.",
                    ));
                }

                if oo_score == cg_score {
                    return Ok(Some(
                        "Error: OO and CG have the same sum of speaks.",
                    ));
                }

                if oo_score == co_score {
                    return Ok(Some(
                        "Error: OO and CO have the same sum of speaks.",
                    ));
                }

                if cg_score == co_score {
                    return Ok(Some(
                        "Error: CG and CO have the same sum of speaks.",
                    ));
                }

                Ok(None)
            })()?;

            let previous_ballot = {
                let previous_ballot_id = adjudicator_ballots::table
                    .filter(adjudicator_ballots::room_id.eq(room.inner.id))
                    .inner_join(spar_adjudicators::table)
                    .filter(spar_adjudicators::member_id.eq(key.member_id))
                    .select(adjudicator_ballots::id)
                    .first::<i64>(conn)
                    .optional()?;
                if let Some(previous_ballot_id) = previous_ballot_id {
                    Some(BallotRepr::of_id(previous_ballot_id, conn)?)
                } else {
                    None
                }
            };

            if let Some(ballot_error) = ballot_error {
                return Ok(Err(render_ballot_form(
                    previous_ballot,
                    room,
                    Some(ballot_error),
                    user,
                    false,
                )));
            }

            // if this is the first time that the ballot is being submitted, we
            // check whether it is contrary to ballots submitted by other
            // in this room
            if !ballot.force {
                if let Some(canonical_ballot) =
                    room.inner.canonical_ballot(conn)?
                {
                    let submitted_scoresheet = Scoresheet {
                        teams: vec![
                            TeamScoresheet {
                                speakers: vec![
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.pm, conn,
                                        )?,
                                        score: ballot.pm_score,
                                    },
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.dpm,
                                            conn,
                                        )?,
                                        score: ballot.dpm_score,
                                    },
                                ],
                            },
                            TeamScoresheet {
                                speakers: vec![
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.lo, conn,
                                        )?,
                                        score: ballot.lo_score,
                                    },
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.dlo,
                                            conn,
                                        )?,
                                        score: ballot.dlo_score,
                                    },
                                ],
                            },
                            TeamScoresheet {
                                speakers: vec![
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.mg, conn,
                                        )?,
                                        score: ballot.mg_score,
                                    },
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.gw, conn,
                                        )?,
                                        score: ballot.gw_score,
                                    },
                                ],
                            },
                            TeamScoresheet {
                                speakers: vec![
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.mo, conn,
                                        )?,
                                        score: ballot.mo_score,
                                    },
                                    SpeakerScoresheet {
                                        // todo: we have already computed these above
                                        speaker_id: id_of_speaker_uuid(
                                            &ballot.ow, conn,
                                        )?,
                                        score: ballot.ow_score,
                                    },
                                ],
                            },
                        ],
                    };

                    if submitted_scoresheet != canonical_ballot.scoresheet {
                        return Ok(Err(render_ballot_form(
                            previous_ballot,
                            room,
                            Some(
                                "Note: a ballot with a different result has
                                   already been submitted for this form.",
                            ),
                            user,
                            false,
                        )));
                    }
                }
            }

            let adjudicator_id = spar_adjudicators::table
                // todo: this should point to the spar_adjudicators table
                .filter(spar_adjudicators::member_id.eq(key.member_id))
                .filter(spar_adjudicators::room_id.eq(key.room_id))
                .select(spar_adjudicators::id)
                .first::<i64>(conn)?;

            let ballot_id = diesel::insert_into(adjudicator_ballots::table)
                .values({
                    (
                        adjudicator_ballots::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballots::adjudicator_id.eq(adjudicator_id),
                        adjudicator_ballots::room_id.eq(room.inner.id),
                        adjudicator_ballots::created_at.eq(diesel::dsl::now),
                    )
                })
                .returning(adjudicator_ballots::id)
                .get_result::<i64>(conn)?;

            let n = diesel::insert_into(adjudicator_ballot_entries::table)
                .values(vec![
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.pm, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[0].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.pm_score),
                        adjudicator_ballot_entries::position.eq(0),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.dpm, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[0].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.dpm_score),
                        adjudicator_ballot_entries::position.eq(1),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.lo, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[1].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.lo_score),
                        adjudicator_ballot_entries::position.eq(0),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.dlo, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[1].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.dlo_score),
                        adjudicator_ballot_entries::position.eq(1),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.mg, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[2].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.mg_score),
                        adjudicator_ballot_entries::position.eq(0),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.gw, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[2].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.gw_score),
                        adjudicator_ballot_entries::position.eq(1),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.mo, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[3].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.mo_score),
                        adjudicator_ballot_entries::position.eq(0),
                    ),
                    (
                        adjudicator_ballot_entries::public_id
                            .eq(gen_uuid().to_string()),
                        adjudicator_ballot_entries::ballot_id.eq(ballot_id),
                        adjudicator_ballot_entries::speaker_id
                            .eq(id_of_speaker_uuid(&ballot.ow, conn)?),
                        adjudicator_ballot_entries::team_id
                            .eq(room.teams[3].inner.id),
                        adjudicator_ballot_entries::speak.eq(ballot.ow_score),
                        adjudicator_ballot_entries::position.eq(1),
                    ),
                ])
                .execute(conn)?;
            assert_eq!(n, 8);

            // todo: build this page
            return Ok(Ok(Redirect::to("/ballots/submit/thanks")));
        })
        .unwrap()
    })
    .await
}

#[get("/ballots/view/<ballot_id>")]
/// Displays a
pub async fn view_ballot(
    user: Option<User>,
    db: DbConn,
    ballot_id: String,
) -> Option<Markup> {
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            let ballot = adjudicator_ballots::table
                .filter(adjudicator_ballots::public_id.eq(&ballot_id))
                .first::<AdjudicatorBallot>(conn)
                .optional()?;

            match ballot {
                None => return Ok(None),
                Some(ballot) => {
                    let adjudicator_name = spar_adjudicators::table
                        .filter(spar_adjudicators::id.eq(ballot.adjudicator_id))
                        .inner_join(spar_series_members::table)
                        .select(spar_series_members::name)
                        .first::<String>(conn)?;
                    let repr = BallotRepr::of_id(ballot.id, conn)?;
                    let room = SparRoomRepr::of_id(ballot.room_id, conn)?;

                    let markup = maud::html! {
                        h3 {"Ballot submitted by " (adjudicator_name)}
                        (render_ballot(&room, &repr))
                    };

                    Ok(Some(page_of_body(markup, user)))
                }
            }
        })
        .unwrap()
    })
    .await
}
