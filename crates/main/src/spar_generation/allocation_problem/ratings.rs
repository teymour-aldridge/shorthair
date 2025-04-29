//! Compute ratings for players using the OpenSkill algorithm. This algorithm is
//! more suitable than ELO when estimating the strength of players for games
//! with multi-player teams and multi-team games (such as British Parliamentary
//! debating).
//!
//! Rating players is (rightly) somewhat frowned upon in debating. However, to
//! match people in a pro-am pairing we do need some idea of their relative
//! skill levels. The approach adopted here is to avoid storing the player
//! strength scores and to never publicize them.
//!
//! The advantage of not releasing the scores is that it is also possible to
//! change the rankings algorithm used at any time, without causing a noticeable
//! difference in perception for the end user.

use std::collections::HashMap;

use db::{
    ballot::BpTeam,
    schema::{adjudicator_ballots, spar_rooms, spar_series_members, spars},
    spar::SparRoom,
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use skillratings::{
    weng_lin::{weng_lin_multi_team, WengLinConfig, WengLinRating},
    MultiTeamOutcome,
};

/// Compute scores for each player.
pub fn compute_scores(
    series_id: i64,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> Result<HashMap<i64, f64>, diesel::result::Error> {
    let rooms_with_results = spar_rooms::table
        .inner_join(spars::table)
        .filter(spars::spar_series_id.eq(series_id))
        .inner_join(adjudicator_ballots::table)
        .select(spar_rooms::all_columns)
        .load::<SparRoom>(conn)?;

    // maps speaker ids (as in the database) to their respective scores
    let mut member_ids_to_scores_map = spar_series_members::table
        .filter(spar_series_members::spar_series_id.eq(series_id))
        .select(spar_series_members::id)
        .load::<i64>(conn)?
        .into_iter()
        .map(|speaker_id| (speaker_id, WengLinRating::new()))
        .collect::<HashMap<_, _>>();

    for room in rooms_with_results {
        let ballot = room.canonical_ballot(conn)?.expect(
            "should not be possible for the result to be missing having
            retrieved only rooms with a ballot having this in the previous step
            (are you sure this is running within a transaction?)",
        );
        let room_repr = room.repr(conn)?;

        let teams = &room_repr.teams;
        let (og, oo, cg, co) = {
            let og = teams[0]
                .speakers
                .iter()
                .map(|speaker_id| {
                    member_ids_to_scores_map
                        .get(&room_repr.speakers[speaker_id].member_id)
                        .unwrap()
                        .clone()
                })
                .collect::<Vec<_>>();
            let oo = teams[1]
                .speakers
                .iter()
                .map(|speaker_id| {
                    member_ids_to_scores_map
                        .get(&room_repr.speakers[speaker_id].member_id)
                        .unwrap()
                        .clone()
                })
                .collect::<Vec<_>>();
            let cg = teams[2]
                .speakers
                .iter()
                .map(|speaker_id| {
                    member_ids_to_scores_map
                        .get(&room_repr.speakers[speaker_id].member_id)
                        .unwrap()
                        .clone()
                })
                .collect::<Vec<_>>();
            let co = teams[3]
                .speakers
                .iter()
                .map(|speaker_id| {
                    member_ids_to_scores_map
                        .get(&room_repr.speakers[speaker_id].member_id)
                        .unwrap()
                        .clone()
                })
                .collect::<Vec<_>>();
            (og, oo, cg, co)
        };

        let ranking = ballot.bp_ranking();

        let teams_and_ranks = vec![
            (
                &og[..],
                MultiTeamOutcome::new(
                    ranking
                        .iter()
                        .position(|t| *t == BpTeam::Og)
                        .expect("must have a position for the team og"),
                ),
            ),
            (
                &oo[..],
                MultiTeamOutcome::new(
                    ranking
                        .iter()
                        .position(|t| *t == BpTeam::Oo)
                        .expect("must have a position for the team og"),
                ),
            ),
            (
                &cg[..],
                MultiTeamOutcome::new(
                    ranking
                        .iter()
                        .position(|t| *t == BpTeam::Cg)
                        .expect("must have a position for the team cg"),
                ),
            ),
            (
                &co[..],
                MultiTeamOutcome::new(
                    ranking
                        .iter()
                        .position(|t| *t == BpTeam::Co)
                        .expect("must have a position for the team co"),
                ),
            ),
        ];

        let new_teams =
            weng_lin_multi_team(&teams_and_ranks, &WengLinConfig::default());

        let _update_og = {
            let new_og = &new_teams[0];
            let og_speakers = &teams[0].speakers;
            let speaker_1 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&og_speakers[0]].member_id)
                .unwrap();
            *speaker_1 = new_og[0];
            let speaker_2 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&og_speakers[1]].member_id)
                .unwrap();
            *speaker_2 = new_og[1];
        };

        let _update_oo = {
            let new_oo = &new_teams[1];
            let oo_speakers = &teams[1].speakers;
            let speaker_1 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&oo_speakers[0]].member_id)
                .unwrap();
            *speaker_1 = new_oo[0];
            let speaker_2 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&oo_speakers[1]].member_id)
                .unwrap();
            *speaker_2 = new_oo[1];
        };

        let _update_cg = {
            let new_cg = &new_teams[2];
            let cg_speakers = &teams[2].speakers;
            let speaker_1 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&cg_speakers[0]].member_id)
                .unwrap();
            *speaker_1 = new_cg[0];
            let speaker_2 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&cg_speakers[1]].member_id)
                .unwrap();
            *speaker_2 = new_cg[1];
        };

        let _update_co = {
            let new_co = &new_teams[2];
            let co_speakers = &teams[2].speakers;
            let speaker_1 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&co_speakers[0]].member_id)
                .unwrap();
            *speaker_1 = new_co[0];
            let speaker_2 = member_ids_to_scores_map
                .get_mut(&room_repr.speakers[&co_speakers[1]].member_id)
                .unwrap();
            *speaker_2 = new_co[1];
        };
    }

    Ok(member_ids_to_scores_map
        .into_iter()
        .map(|(id, score)| (id, score.rating))
        .collect())
}

/// Adds debug information about ELO scores (this can be helpful in ensuring
/// that they have been correctly calculated).
pub fn trace_scores(scores: &HashMap<i64, f64>) {
    for (id, score) in scores {
        tracing::trace!("Elo score for user with id {id} is {score}");
    }
}
