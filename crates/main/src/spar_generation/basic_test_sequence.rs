//! A basic test of application functionality. Intended to ensure that all
//! features work correctly on the "happy" path. For more extensive testing
//! there is a model of the application (wired up to a fuzzer).

use crate::auth::login::PasswordLoginForm;
use crate::groups::CreateSparSeriesForm;
use crate::spar_generation::ballots::BpBallotForm;
use crate::spar_generation::individual_spars::signup_routes::SignupForSpar;
use crate::spar_generation::spar_series::admin_routes::{
    ApproveJoinRequestForm, MakeSessionForm, Request2JoinSparSeriesForm,
};
use crate::{
    auth::register::RegisterForm, groups::CreateGroupForm, make_rocket,
};

use db::ballot::AdjudicatorBallotLink;
use db::draft_draw::DraftDraw;
use db::email::EmailRow;
use db::schema::{
    draft_draws, emails, spar_adjudicator_ballot_links, spar_rooms,
    spar_series, spar_series_join_requests, spar_series_members, spars,
};
use db::spar::{Spar, SparRoom, SparSeries, SparSeriesMember};
use db::{group::Group, schema::groups};
use diesel::connection::LoadConnection;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use diesel::SqliteConnection;
use rocket::http::ContentType;
use rocket::local::blocking::Client;
use uuid::Uuid;

fn get_test_rocket_instance(
) -> (rocket::local::blocking::Client, SqliteConnection) {
    use std::sync::Arc;

    use diesel::{Connection, RunQueryDsl};

    let db_name = Arc::new(format!("{}.db", Uuid::now_v7()));

    let mut conn = diesel::SqliteConnection::establish(&db_name.to_string())
        .expect("Database connection failed");
    diesel::sql_query("PRAGMA journal_mode=WAL")
        .execute(&mut conn)
        .expect("Failed to enable WAL mode");
    diesel::sql_query("PRAGMA foreign_keys=ON")
        .execute(&mut conn)
        .expect("Failed to enable foreign keys");
    diesel::sql_query("pragma synchronous = off;")
        .execute(&mut conn)
        .expect("Failed to disable sync commit foreign keys");

    let rocket = make_rocket(&db_name.clone());
    (Client::tracked(rocket).unwrap(), conn)
}

#[test]
fn basic_test_sequence() {
    const PASSWORD: &str = "random@string123!!:";

    let (rocket, mut conn) = get_test_rocket_instance();

    // (1) register
    rocket
        .post("/register")
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&RegisterForm {
                username: "user".to_string(),
                email: "user@example.com".to_string(),
                password: PASSWORD.to_string(),
                password2: PASSWORD.to_string(),
            })
            .unwrap(),
        )
        .dispatch();

    rocket
        .post("/login")
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&PasswordLoginForm {
                email: "user@example.com".to_string(),
                password: PASSWORD.to_string(),
            })
            .unwrap(),
        )
        .dispatch();

    // (2) create group

    rocket
        .post("/groups/new")
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&CreateGroupForm {
                name: "Group".to_string(),
                website: Some("https://example.com".to_string()),
            })
            .unwrap(),
        )
        .dispatch();

    // (4) create spar series and spar

    let group = groups::table
        .order_by(groups::created_at.desc())
        .first::<Group>(&mut conn)
        .unwrap();

    rocket
        .post(format!("/groups/{}/spar_series/new", group.public_id))
        .header(ContentType::Form)
        .body(
            serde_urlencoded::to_string(&CreateSparSeriesForm {
                title: "Spar series".to_string(),
                description: Some("The spar series".to_string()),
            })
            .unwrap(),
        )
        .dispatch();

    let spar_series = spar_series::table
        .order_by(spar_series::created_at.desc())
        .first::<SparSeries>(&mut conn)
        .unwrap();

    rocket
        .post(format!("/spar_series/{}/makesess", spar_series.public_id))
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&MakeSessionForm {
                start_time: chrono::Utc::now()
                    .checked_add_days(chrono::Days::new(2))
                    .unwrap()
                    .naive_utc()
                    .format("%Y-%m-%dT%H:%M")
                    .to_string(),
                is_open: Some("true".to_string()),
            })
            .unwrap(),
        )
        .dispatch();

    let spar = spars::table
        .order_by(spars::created_at.desc())
        .first::<Spar>(&mut conn)
        .unwrap();

    rocket.post("/logout").dispatch();

    let _do_member_join_requests = {
        for i in 1..=8 {
            rocket
                .post(format!(
                    "/spar_series/{}/request2join",
                    spar_series.public_id
                ))
                .header(ContentType::Form)
                .body(
                    &serde_urlencoded::to_string(&Request2JoinSparSeriesForm {
                        name: format!("Speaker{i}"),
                        email: format!("speaker{i}@example.com"),
                    })
                    .unwrap(),
                )
                .dispatch();
        }
        rocket
            .post(format!(
                "/spar_series/{}/request2join",
                spar_series.public_id
            ))
            .header(ContentType::Form)
            .body(
                &serde_urlencoded::to_string(&Request2JoinSparSeriesForm {
                    name: "Judge1".to_owned(),
                    email: "judge1@example.com".to_owned(),
                })
                .unwrap(),
            )
            .dispatch();
    };

    rocket
        .post("/login")
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&PasswordLoginForm {
                email: "user@example.com".to_string(),
                password: PASSWORD.to_string(),
            })
            .unwrap(),
        )
        .dispatch();

    let _approve_member_join_requests = {
        let join_requests = spar_series_join_requests::table
            .order_by(spar_series_join_requests::created_at.desc())
            .load::<SparSeriesMember>(&mut conn)
            .unwrap();
        assert_eq!(join_requests.len(), 9);
        for req in join_requests {
            rocket
                .post(format!(
                    "/spar_series/{}/approve_join_request",
                    spar_series.public_id
                ))
                .header(ContentType::Form)
                .body(
                    &serde_urlencoded::to_string(ApproveJoinRequestForm {
                        id: req.public_id.parse().unwrap(),
                    })
                    .unwrap(),
                )
                .dispatch();
        }
    };

    rocket.get("/logout");

    // (4)(a) signups

    let members = spar_series_members::table
        .order_by(spar_series_members::created_at.desc())
        .load::<SparSeriesMember>(&mut conn)
        .unwrap();
    assert_eq!(members.len(), 9);

    for member in &members {
        let (as_judge, as_speaker, partner_preference) =
            if member.name.to_ascii_lowercase().contains("speaker") {
                (
                    false,
                    true,
                    match member.name.as_str() {
                        "Speaker1" => Some(
                            members
                                .iter()
                                .find(|member| member.name == "Speaker2")
                                .map(|member| member.public_id.clone())
                                .unwrap(),
                        ),
                        "Speaker3" => Some(
                            members
                                .iter()
                                .find(|member| member.name == "Speaker4")
                                .map(|member| member.public_id.clone())
                                .unwrap(),
                        ),
                        _ => None,
                    },
                )
            } else {
                assert!(member.name.to_ascii_lowercase().contains("judge"));
                (true, false, None)
            };
        rocket
            .post(format!(
                "/spars/{}/signup/{}",
                spar.public_id, member.public_id
            ))
            .header(ContentType::Form)
            .body(
                serde_urlencoded::to_string(&SignupForSpar {
                    as_judge,
                    as_speaker,
                    speaking_partner: partner_preference
                        .map(|t| t.parse().unwrap()),
                })
                .unwrap(),
            )
            .dispatch();
    }

    // (4)(b) start spar

    rocket
        .post("/login")
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&PasswordLoginForm {
                email: "user@example.com".to_string(),
                password: PASSWORD.to_string(),
            })
            .unwrap(),
        )
        .dispatch();

    rocket
        .post(format!("/spars/{}/makedraw", spar.public_id))
        .dispatch();

    let mut draft = draft_draws::table
        .order_by(draft_draws::created_at.desc())
        .first::<DraftDraw>(&mut conn)
        .optional()
        .unwrap();

    while draft.is_none()
        || (draft.is_some() && draft.as_ref().unwrap().data.is_none())
    {
        std::thread::sleep(std::time::Duration::from_secs(1));
        draft = draft_draws::table
            .order_by(draft_draws::created_at.desc())
            .first::<DraftDraw>(&mut conn)
            .optional()
            .unwrap();
    }

    let draft = draft.unwrap();

    rocket
        .post(format!(
            "/spars/{}/draws/{}/confirm",
            spar.public_id, draft.public_id
        ))
        .header(ContentType::Form)
        .dispatch();

    rocket
        .post(format!(
            "/spars/{}/set_released?released=true",
            spar.public_id
        ))
        .header(ContentType::Form)
        .dispatch();

    let rooms = spar_rooms::table
        .filter(spar_rooms::spar_id.eq(spar.id))
        .load::<SparRoom>(&mut conn)
        .unwrap();

    assert_eq!(rooms.len(), 1);
    let room = &rooms[0];

    let _verify_partner_preferences = {
        // Verify that speaker1 and speaker2 are on the same team, same for speaker3 and speaker4
        let repr = room.repr(&mut conn).unwrap();
        let members = spar_series_members::table
            .load::<SparSeriesMember>(&mut conn)
            .unwrap();

        // Find speaker1, speaker2, speaker3, speaker4 member IDs
        let find_member_id = |name: &str| -> i64 {
            members
                .iter()
                .find(|m| m.name == name)
                .expect(&format!("Could not find member with name {}", name))
                .id
        };

        let speaker1_id = find_member_id("Speaker1");
        let speaker2_id = find_member_id("Speaker2");
        let speaker3_id = find_member_id("Speaker3");
        let speaker4_id = find_member_id("Speaker4");

        // Find which team each speaker is on
        let find_team_for_speaker = |speaker_id: i64| -> i64 {
            for team in &repr.teams {
                for speaker_db_id in &team.speakers {
                    let speaker = &repr.speakers[speaker_db_id];
                    if repr.members[&speaker.member_id].id == speaker_id {
                        return team.inner.id;
                    }
                }
            }
            panic!("Could not find team for speaker ID {}", speaker_id);
        };

        let speaker1_team = find_team_for_speaker(speaker1_id);
        let speaker2_team = find_team_for_speaker(speaker2_id);
        let speaker3_team = find_team_for_speaker(speaker3_id);
        let speaker4_team = find_team_for_speaker(speaker4_id);

        // Assert that speaker1 and speaker2 are on the same team
        assert_eq!(
            speaker1_team, speaker2_team,
            "Speaker1 and Speaker2 should be on the same team"
        );

        // Assert that speaker3 and speaker4 are on the same team
        assert_eq!(
            speaker3_team, speaker4_team,
            "Speaker3 and Speaker4 should be on the same team"
        );
    };

    // (4)(c) ensure that all emails are sent

    std::thread::sleep(std::time::Duration::from_millis(50));
    let emails = emails::table.load::<EmailRow>(&mut conn).unwrap();
    assert_eq!(emails.len(), 1);

    assert!(emails[0]
        .contents
        .as_ref()
        .map(|contents| contents.contains("https://"))
        .unwrap_or(false));

    submit_ballot(&rocket, &mut conn);

    // (5) conclude spar

    mark_spar_complete(&rocket, spar);
    let spar = spars::table.first::<Spar>(&mut conn).unwrap();
    assert!(spar.is_complete);

    assert_eq!(
        spar_rooms::table
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        1
    );

    let room = spar_rooms::table.first::<SparRoom>(&mut conn).unwrap();
    let repr = room.repr(&mut conn).unwrap();
    let teams = repr.teams;

    let pm_id = repr.speakers[&teams[0].speakers[0]].public_id.clone();
    let dpm_id = repr.speakers[&teams[0].speakers[1]].public_id.clone();

    let submitted_ballot = room.canonical_ballot(&mut conn).unwrap().unwrap();
    let og = &submitted_ballot.scoresheet.teams[0];
    let pm = &og.speakers[0];
    let dpm = &og.speakers[1];
    assert_eq!(pm.score, 80);
    assert_eq!(repr.speakers[&pm.speaker_id].public_id, pm_id);
    assert_eq!(dpm.score, 78);
    assert_eq!(repr.speakers[&dpm.speaker_id].public_id, dpm_id);

    // (6) create second spar

    assert_eq!(
        spars::table.count().get_result::<i64>(&mut conn).unwrap(),
        1
    );

    rocket
        .post(format!("/spar_series/{}/makesess", spar_series.public_id))
        .header(ContentType::Form)
        .body(
            &serde_urlencoded::to_string(&MakeSessionForm {
                start_time: chrono::Utc::now()
                    .checked_add_days(chrono::Days::new(2))
                    .unwrap()
                    .naive_utc()
                    .format("%Y-%m-%dT%H:%M")
                    .to_string(),
                is_open: Some("true".to_string()),
            })
            .unwrap(),
        )
        .dispatch();

    assert_eq!(
        spars::table.count().get_result::<i64>(&mut conn).unwrap(),
        2
    );

    let spar = spars::table
        .order_by(spars::created_at.desc())
        .first::<Spar>(&mut conn)
        .unwrap();

    let members = spar_series_members::table
        .order_by(spar_series_members::created_at.desc())
        .load::<SparSeriesMember>(&mut conn)
        .unwrap();
    assert_eq!(members.len(), 9);

    for member in members {
        let (as_judge, as_speaker) =
            if member.name.to_ascii_lowercase().contains("speaker") {
                (false, true)
            } else {
                assert!(member.name.to_ascii_lowercase().contains("judge"));
                (true, false)
            };
        rocket
            .post(format!(
                "/spars/{}/signup/{}",
                spar.public_id, member.public_id
            ))
            .header(ContentType::Form)
            .body(
                serde_urlencoded::to_string(&SignupForSpar {
                    as_judge,
                    as_speaker,
                    speaking_partner: None,
                })
                .unwrap(),
            )
            .dispatch();
    }

    rocket
        .post(format!("/spars/{}/makedraw", spar.public_id))
        .dispatch();

    let mut draft = draft_draws::table
        .order_by(draft_draws::created_at.desc())
        .first::<DraftDraw>(&mut conn)
        .optional()
        .unwrap();

    while draft.is_none()
        || (draft.is_some() && draft.as_ref().unwrap().data.is_none())
    {
        std::thread::sleep(std::time::Duration::from_secs(1));
        draft = draft_draws::table
            .order_by(draft_draws::created_at.desc())
            .first::<DraftDraw>(&mut conn)
            .optional()
            .unwrap();
    }

    let draft = draft.unwrap();

    rocket
        .post(format!(
            "/spars/{}/draws/{}/confirm",
            spar.public_id, draft.public_id
        ))
        .header(ContentType::Form)
        .dispatch();

    rocket
        .post(format!(
            "/spars/{}/set_released?released=true",
            spar.public_id
        ))
        .header(ContentType::Form)
        .dispatch();

    rocket
        .post(format!(
            "/spars/{}/set_released?released=true",
            spar.public_id
        ))
        .dispatch();

    assert_eq!(
        spar_rooms::table
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        2
    );
}

fn mark_spar_complete(rocket: &Client, spar: Spar) {
    rocket
        .post(format!("/spars/{}/mark_complete", spar.public_id))
        .dispatch();
}

fn submit_ballot(
    rocket: &Client,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) {
    let ballot_link = spar_adjudicator_ballot_links::table
        .order_by(spar_adjudicator_ballot_links::created_at.desc())
        .first::<AdjudicatorBallotLink>(conn)
        .unwrap();

    let room = spar_rooms::table.first::<SparRoom>(conn).unwrap();
    assert_eq!(room.id, ballot_link.room_id);

    let repr = room.repr(conn).unwrap();
    let teams = repr.teams;

    let pm_id = repr.speakers[&teams[0].speakers[0]].public_id.clone();
    let dpm_id = repr.speakers[&teams[0].speakers[1]].public_id.clone();
    let submission = BpBallotForm {
        pm: pm_id.clone(),
        pm_score: 80,
        dpm: dpm_id.clone(),
        dpm_score: 78,
        lo: repr.speakers[&teams[1].speakers[0]].public_id.clone(),
        lo_score: 76,
        dlo: repr.speakers[&teams[1].speakers[1]].public_id.clone(),
        dlo_score: 75,
        mg: repr.speakers[&teams[2].speakers[0]].public_id.clone(),
        mg_score: 77,
        gw: repr.speakers[&teams[2].speakers[1]].public_id.clone(),
        gw_score: 73,
        mo: repr.speakers[&teams[3].speakers[0]].public_id.clone(),
        mo_score: 73,
        ow: repr.speakers[&teams[3].speakers[1]].public_id.clone(),
        ow_score: 72,
        force: false,
    };

    rocket
        .post(format!("/ballots/submit/{}", ballot_link.link))
        .header(ContentType::Form)
        .body(&serde_urlencoded::to_string(&submission).unwrap())
        .dispatch();
}
