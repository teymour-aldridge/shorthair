use std::{collections::HashMap, time::Duration};

use regex::Regex;
use rocket::tokio::time::sleep;
use tabbycat_api::{types::Speaker, Client};

async fn pause() {
    sleep(Duration::from_millis(10)).await;
}

/// Contains speaks for each speaker in the round in question.
pub struct Room {
    og: (usize, usize),
    oo: (usize, usize),
    co: (usize, usize),
    cg: (usize, usize),
    og_speaks: (u8, u8),
    oo_speaks: (u8, u8),
    cg_speaks: (u8, u8),
    co_speaks: (u8, u8),
}

// struct Rounds(Vec<Room>);

/// Loads rooms from Tabbycat.
///
/// Note that an important invariant is
///     round(room1).time <= round(room2).time
///     => index of room1 <= index of room2
pub async fn load_rounds_from_tabbycat(
    client: &Client,
    tournament_slug: &str,
) -> (Vec<Room>, HashMap<i64, Speaker>) {
    let mut rooms = Vec::new();
    let rounds = client
        .api_v1_tournaments_rounds_list(tournament_slug, None, None)
        .await
        .unwrap()
        .into_inner()
        .0;

    pause().await;

    for round in rounds {
        if round.completed != Some(true) {
            // todo: log and return
            continue;
        }
        let round_seq = round.seq;
        let debates = client
            .api_v1_tournaments_rounds_pairings_list(
                tournament_slug,
                round_seq,
                None,
                None,
            )
            .await
            .unwrap()
            .into_inner();
        pause().await;

        for debate in debates.iter() {
            let ballots = client
                .api_v1_tournaments_rounds_pairings_ballots_list(
                    tournament_slug,
                    round_seq,
                    debate.id,
                    Some(true),
                    None,
                    None,
                )
                .await
                .unwrap()
                .into_inner()
                .0;
            // ensure we wait at least 10ms between requests
            pause().await;

            // todo: log and warn if there are multiple ballots
            let ballot = ballots[0].clone();
            let teams = ballot.result.sheets[0].teams.clone();

            // todo: this needs to be fixed (as we are using 8 teams)
            let og = teams[0].clone();
            let oo = teams[1].clone();
            let cg = teams[2].clone();
            let co = teams[3].clone();

            rooms.push(Room {
                og: (
                    team_url_to_id(&og.speeches[0].speaker),
                    team_url_to_id(&og.speeches[1].speaker),
                ),
                oo: (
                    team_url_to_id(&oo.speeches[0].speaker),
                    team_url_to_id(&oo.speeches[1].speaker),
                ),
                cg: (
                    team_url_to_id(&cg.speeches[0].speaker),
                    team_url_to_id(&cg.speeches[1].speaker),
                ),
                co: (
                    team_url_to_id(&co.speeches[0].speaker),
                    team_url_to_id(&co.speeches[1].speaker),
                ),
                og_speaks: (
                    og.speeches[0].score as u8,
                    og.speeches[1].score as u8,
                ),
                oo_speaks: (
                    oo.speeches[0].score as u8,
                    oo.speeches[1].score as u8,
                ),
                cg_speaks: (
                    cg.speeches[0].score as u8,
                    cg.speeches[1].score as u8,
                ),
                co_speaks: (
                    co.speeches[0].score as u8,
                    co.speeches[1].score as u8,
                ),
            });
        }
    }

    let speakers = client
        .api_v1_tournaments_speakers_list(tournament_slug, None, None)
        .await
        .unwrap()
        .0
        .clone()
        .into_iter()
        .map(|speaker| (speaker.id, speaker.clone()))
        .collect::<HashMap<i64, _>>();
    (rooms, speakers)
}

fn team_url_to_id(input: &str) -> usize {
    let re = Regex::new(r"(\d+)$").unwrap();
    re.captures(input)
        .and_then(|caps| {
            caps.get(1).map(|m| m.as_str().parse::<usize>().unwrap())
        })
        .unwrap()
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct SpeakerId(pub usize);

/// Computes the mapping SPEAKER_ID -> ELO_SCORE.
///
/// An ELO score provides an indication of how strong a player is; higher is
/// better. Players start with an ELO of 1500.0, lose points when they lose to
/// other players and gain points when they win against other players.
///
/// We use the approach detailed in
/// https://monashdebaters.com/introducing-elo-ratings-in-british-parliamentary-debating
pub async fn compute_elo(rooms: Vec<Room>) -> HashMap<SpeakerId, f64> {
    let mut elo_scores = HashMap::new();

    for room in rooms {
        let og = TeamWithElo {
            speaker_1_speaks: room.og_speaks.0,
            speaker_1_elo_before: *elo_scores
                .get(&room.og.0)
                .unwrap_or(&1500.0),
            speaker_2_speaks: room.og_speaks.1,
            speaker_2_elo_before: *elo_scores
                .get(&room.og.1)
                .unwrap_or(&1500.0),
        };
        let oo = TeamWithElo {
            speaker_1_speaks: room.oo_speaks.0,
            speaker_1_elo_before: *elo_scores
                .get(&room.oo.0)
                .unwrap_or(&1500.0),
            speaker_2_speaks: room.oo_speaks.1,
            speaker_2_elo_before: *elo_scores
                .get(&room.oo.1)
                .unwrap_or(&1500.0),
        };
        let cg = TeamWithElo {
            speaker_1_speaks: room.cg_speaks.0,
            speaker_1_elo_before: *elo_scores
                .get(&room.cg.0)
                .unwrap_or(&1500.0),
            speaker_2_speaks: room.cg_speaks.1,
            speaker_2_elo_before: *elo_scores
                .get(&room.cg.1)
                .unwrap_or(&1500.0),
        };
        let co = TeamWithElo {
            speaker_1_speaks: room.co_speaks.0,
            speaker_1_elo_before: *elo_scores
                .get(&room.co.0)
                .unwrap_or(&1500.0),
            speaker_2_speaks: room.co_speaks.1,
            speaker_2_elo_before: *elo_scores
                .get(&room.co.1)
                .unwrap_or(&1500.0),
        };

        let mut og_change = 0.0;
        let mut oo_change = 0.0;
        let mut cg_change = 0.0;
        let mut co_change = 0.0;

        let (x, y) = compute_elo_change(og, oo);
        og_change += x;
        oo_change += y;
        let (x, y) = compute_elo_change(og, cg);
        og_change += x;
        cg_change += y;
        let (x, y) = compute_elo_change(og, co);
        og_change += x;
        co_change += y;
        let (x, y) = compute_elo_change(oo, cg);
        oo_change += x;
        cg_change += y;
        let (x, y) = compute_elo_change(oo, co);
        oo_change += x;
        co_change += y;
        let (x, y) = compute_elo_change(cg, co);
        cg_change += x;
        co_change += y;

        let mut modify = |speaker, change| {
            elo_scores
                .entry(speaker)
                .and_modify(|score| *score += og_change)
                // todo: might want to log this
                .or_insert(1500.0 + change);
        };

        modify(room.og.0, og_change);
        modify(room.og.1, og_change);
        modify(room.oo.0, oo_change);
        modify(room.oo.0, oo_change);
        modify(room.cg.0, cg_change);
        modify(room.cg.1, cg_change);
        modify(room.co.0, co_change);
        modify(room.co.0, co_change);
    }

    elo_scores
        .into_iter()
        .map(|(id, score)| (SpeakerId(id), score))
        .collect()
}

#[derive(Copy, Clone)]
pub struct TeamWithElo {
    speaker_1_speaks: u8,
    speaker_1_elo_before: f64,
    speaker_2_speaks: u8,
    speaker_2_elo_before: f64,
}

pub fn compute_elo_change(
    team_1: TeamWithElo,
    team_2: TeamWithElo,
) -> (f64, f64) {
    let team_1_wins = team_1.speaker_1_speaks + team_1.speaker_2_speaks
        > team_2.speaker_1_speaks + team_2.speaker_2_speaks;

    let team_1_rating =
        (team_1.speaker_1_elo_before + team_1.speaker_2_elo_before) / 2.0;
    let team_2_rating =
        (team_2.speaker_1_elo_before + team_2.speaker_2_elo_before) / 2.0;

    let expected_team_1 =
        1.0 / (1.0 + (10.0_f64).powf((team_2_rating - team_1_rating) / 400.0));
    let expected_team_2 =
        1.0 / (1.0 + (10.0_f64).powf((team_1_rating - team_2_rating) / 400.0));

    let team_1_rating_change =
        32.0 * (if team_1_wins { 1.0 } else { 0.0 } - expected_team_1);

    let team_2_rating_change =
        32.0 * (if !team_1_wins { 1.0 } else { 0.0 } - expected_team_2);

    (team_1_rating_change, team_2_rating_change)
}
