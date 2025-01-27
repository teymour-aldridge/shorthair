use std::collections::HashMap;

/// Contains speaks for each speaker in the round in question.
// todo: use the native SQL types
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
