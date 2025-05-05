//! Generates a draw by formulating the problem as an ILP, and then solves
//! this.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use db::spar::SparSignup;
use good_lp::{
    constraint, variables, Expression, Solution, SolverModel,
    VariableDefinition,
};
use serde::{Deserialize, Serialize};

/// Always remember: if it runs in polynomial time, it's efficient (for
/// constructing the problem instance).
#[tracing::instrument]
pub fn solve_lp(
    person_and_signup_data: Arc<HashMap<i64, SparSignup>>,
    elo_scores: HashMap<i64, f64>,
) -> HashMap<i64, Assignment> {
    // maximum number of rooms (where everyone is assigned to speak)
    // todo: this number can be reduced
    let r_max = person_and_signup_data
        .iter()
        .filter(|(_id, signup)| signup.as_speaker)
        .count()
        .div_ceil(8);

    let mut vars = variables!();

    // create the variables
    //
    // the names are as in the descriptions above
    let (x_irj, u_r) = {
        let mut x_irj = HashMap::new();
        let mut u_r = HashMap::new();
        // set up variables
        for (participant_id, _) in person_and_signup_data.iter() {
            for room_idx in 0..r_max {
                // create u_r
                let room = vars.add(
                    VariableDefinition::new()
                        .binary()
                        .name(format!("u({room_idx})")),
                );
                u_r.insert(room_idx, room);
                // create x_{i, r, j}
                //
                // note: roles
                // 0 = OG
                // 1 = OO
                // 2 = CG
                // 3 = CO
                // 4 = judge
                for role in 0..5usize {
                    let variable =
                        vars.add(VariableDefinition::new().binary().name(
                            format!("x({participant_id}, {room_idx}, {role})"),
                        ));
                    x_irj.insert((participant_id, room_idx, role), variable);
                }
            }
        }

        (x_irj, u_r)
    };

    // gather constraints
    let mut constraints = Vec::new();

    let () = {
        for participant_id in person_and_signup_data.keys() {
            let record = &person_and_signup_data[participant_id];

            // add judge/speaker constraints
            for room in 0..r_max {
                // if not signed up as a judge, then we add a constraint which
                // prevents the user from being allocated as a judge
                assert!(record.as_judge || record.as_speaker);
                if !record.as_judge {
                    let constraint = constraint! {
                        x_irj[&(participant_id, room, 4)] <= 0
                    };
                    constraints.push(constraint);
                }

                if !record.as_speaker {
                    for role in 0..=3usize {
                        let constraint = constraint! {
                            x_irj[&(participant_id, room, role)] <= 0
                        };
                        constraints.push(constraint);
                    }
                }
            }

            tracing::trace!(
                "Number of variables for judge/speaker constraints: {}",
                vars.len()
            );
            tracing::trace!(
                "Number of constraints for judge/speaker constraints: {}",
                constraints.len()
            );

            let mut positions_allocated = Expression::default();
            for room in 0..r_max {
                for role in 0..=4usize {
                    positions_allocated += x_irj[&(participant_id, room, role)];
                }
            }
            constraints.push(constraint! {
                positions_allocated.clone() >= 1
            });
            constraints.push(constraint! {
                positions_allocated <= 1
            });

            tracing::trace!(
                "Number of variables for position constraints: {}",
                vars.len()
            );
            tracing::trace!(
                "Number of constraints for position constraints: {}",
                constraints.len()
            );
        }
    };

    let () = {
        for room in 0..r_max {
            let mut judge_count = Expression::default();
            let mut og_count = Expression::default();
            let mut oo_count = Expression::default();
            let mut cg_count = Expression::default();
            let mut co_count = Expression::default();

            for participant_id in person_and_signup_data.keys() {
                judge_count += x_irj[&(participant_id, room, 4)];
                og_count += x_irj[&(participant_id, room, 0)];
                oo_count += x_irj[&(participant_id, room, 1)];
                cg_count += x_irj[&(participant_id, room, 2)];
                co_count += x_irj[&(participant_id, room, 3)];
            }

            constraints.push(constraint!(judge_count.clone() >= u_r[&room]));

            // ensure that judges are not allocated into inactive rooms
            constraints
                .push(constraint!(judge_count.clone() <= 100 * u_r[&room]));

            constraints.push(constraint!(og_count.clone() <= 2 * u_r[&room]));
            constraints.push(constraint!(og_count.clone() >= u_r[&room]));

            constraints.push(constraint!(oo_count.clone() <= 2 * u_r[&room]));
            constraints.push(constraint!(oo_count.clone() >= u_r[&room]));

            constraints.push(constraint!(cg_count.clone() <= 2 * u_r[&room]));
            constraints.push(constraint!(cg_count.clone() >= u_r[&room]));

            constraints.push(constraint!(co_count.clone() <= 2 * u_r[&room]));
            constraints.push(constraint!(co_count.clone() >= u_r[&room]));

            tracing::trace!(
                "Number of variables for room constraints: {}",
                vars.len()
            );
            tracing::trace!(
                "Number of constraints for room constraints: {}",
                constraints.len()
            );
        }
    };

    let score_per_team = {
        let mut score_per_team = HashMap::new();

        for room_idx in 0..r_max {
            for role in 0..=3 {
                // efficiency... what does this word "efficiency" mean?

                let rating_of_speakers = x_irj
                    .iter()
                    .filter(
                        |(
                            (
                                /* we want to select all the participants */
                                _i,
                                r,
                                j,
                            ),
                            _,
                        )| {
                            *r == room_idx && *j == role
                        },
                    )
                    .map(|((i, _j, _r), lp_variable)| {
                        let score: Expression = (*lp_variable)
                            // todo: assert that we never pick unwrap
                            * (*elo_scores.get(i).unwrap_or(&25.0));
                        score
                    })
                    .collect::<Vec<_>>();

                score_per_team.insert((room_idx, role), rating_of_speakers);
            }
        }

        score_per_team
    };

    // minimise difference between teams
    let difference_between_teams = {
        // ELO score of each team
        //
        // this is in the form (room_idx, role)

        let mut difference_between_teams = Expression::default();
        // here we compute the average speaker score of each team
        for room_idx in 0..r_max {
            for team_1_pos in 0..=3 {
                for team_2_pos in (team_1_pos + 1)..=3 {
                    let team_1 = score_per_team[&(room_idx, team_1_pos)]
                        .clone()
                        .iter()
                        .map(|exp| exp.clone())
                        .sum::<Expression>();
                    let team_2 = score_per_team[&(room_idx, team_2_pos)]
                        .clone()
                        .iter()
                        .map(|exp| exp.clone())
                        .sum::<Expression>();

                    // See this for an explanation:
                    // https://math.stackexchange.com/questions/1954992
                    let absolute_value_of_difference =
                        vars.add(VariableDefinition::new());
                    let diff: Expression =
                        (team_1.clone() - team_2.clone()) / 2.0;
                    let diff_neg: Expression = -1.0 * (team_1 - team_2) / 2.0;
                    constraints.push(constraint!(
                        absolute_value_of_difference >= diff
                    ));
                    constraints.push(constraint!(
                        absolute_value_of_difference >= diff_neg
                    ));
                    difference_between_teams += absolute_value_of_difference;
                }
            }
        }

        difference_between_teams
    };

    tracing::trace!(
        "Number of variables after difference between teams: {}",
        vars.len()
    );
    tracing::trace!(
        "Number of constraints after difference between teams: {}",
        constraints.len()
    );

    // Adds an optimisation term that reduces the difference between speakers
    // on a team. We constraint `max_rating_on_team` and `min_rating_on_team`
    // to be greater than (or less than) all speakers on the team, and then
    // seek to minimise
    //
    // max - min
    //
    // (summing over this for all speakers).
    //
    // todo: this will need adapting for 3v3 spar generation (when this is
    // eventually supported)
    let difference_between_speakers = {
        let max_rating_of_all_speakers =
            elo_scores.values().max_by(|a, b| a.total_cmp(b)).unwrap();
        let min_rating_of_all_speakers =
            elo_scores.values().min_by(|a, b| a.total_cmp(b)).unwrap();

        let mut difference_between_speakers = Expression::default();

        for room_idx in 0..r_max {
            for team in 0..=3 {
                let max_rating_on_team = vars.add(VariableDefinition::new());
                let min_rating_on_team = vars.add(VariableDefinition::new());

                constraints.push(constraint!(
                    max_rating_on_team <= (*max_rating_of_all_speakers + 100.0)
                ));
                constraints.push(constraint!(
                    (*min_rating_of_all_speakers - 100.0) <= min_rating_on_team
                ));

                for participant in person_and_signup_data.keys() {
                    let score: Expression = x_irj
                        .get(&(participant, room_idx, team))
                        .unwrap()
                        .clone()
                        * elo_scores.get(participant).unwrap().clone();
                    constraints
                        .push(constraint!(score.clone() <= max_rating_on_team));
                    constraints
                        .push(constraint!(min_rating_on_team <= score.clone()));
                }

                difference_between_speakers +=
                    max_rating_on_team - min_rating_on_team;
            }
        }

        difference_between_speakers
    };

    tracing::trace!(
        "Number of variables after difference between speakers: {}",
        vars.len()
    );
    tracing::trace!(
        "Number of constraints after difference between speakers: {}",
        constraints.len()
    );

    // we want fewer rooms (where possible)
    let fewer_rooms_objective = {
        let mut room_count = Expression::default();

        for i in 0..r_max {
            room_count += u_r[&i];
        }

        room_count
    };

    let judge_penalty = {
        let difference_between_rooms = {
            let mut difference_between_rooms = Expression::default();

            // For each room, calculate how many judges are allocated
            let mut judge_counts = Vec::new();
            for room_idx in 0..r_max {
                let mut judge_count = Expression::default();
                for participant_id in person_and_signup_data.keys() {
                    judge_count += x_irj[&(participant_id, room_idx, 4)];
                }
                judge_counts.push(judge_count);
            }

            for i in 0..r_max {
                for j in (i + 1)..r_max {
                    let diff_judge_count_room_i_and_judge_count_room_j =
                        vars.add(VariableDefinition::new());
                    let diff =
                        judge_counts[i].clone() - judge_counts[j].clone();

                    // |a-b| = max(a-b, b-a)
                    constraints.push(constraint!(
                        diff_judge_count_room_i_and_judge_count_room_j
                            >= diff.clone()
                    ));
                    constraints.push(constraint!(
                        diff_judge_count_room_i_and_judge_count_room_j
                            >= -1.0 * diff
                    ));

                    difference_between_rooms +=
                        diff_judge_count_room_i_and_judge_count_room_j;
                }
            }

            difference_between_rooms
        };

        let num_judges = {
            let mut num_judges = Expression::default();

            for room_idx in 0..r_max {
                for participant_id in person_and_signup_data.keys() {
                    num_judges += x_irj[&(participant_id, room_idx, 4)];
                }
            }

            num_judges
        };

        difference_between_rooms + num_judges
    };

    let partner_preferences = {
        let mut expr = Expression::default();
        for (participant_id, signup) in person_and_signup_data.iter() {
            if let Some(preferred_partner) = signup.partner_preference {
                for r in 0..r_max {
                    for j in 0..=3 {
                        let z = vars.add(VariableDefinition::new());
                        let x = x_irj[&(participant_id, r, j)];
                        let y = x_irj[&(&preferred_partner, r, j)];
                        constraints.push(constraint!(z <= x));
                        constraints.push(constraint!(z <= y));
                        constraints.push(constraint!(x >= x + y - 1));
                        expr += z;
                    }
                }
            }
        }
        expr
    };

    tracing::info!("Number of variables for problem: {}", vars.len());
    tracing::info!("Number of constraints for problem: {}", constraints.len());

    let mut problem = vars
        .maximise(
            // todo: work out what the right multipliers are
            (-1 * judge_penalty)
                + (-1 * difference_between_teams)
                + 1 * (difference_between_speakers)
                + (-5 * fewer_rooms_objective)
                + 20 * partner_preferences,
        )
        .using(good_lp::solvers::highs::highs);

    // add constraints to problem
    for constraint in constraints {
        problem = problem.with(constraint);
    }

    tracing::trace!("Constructed problem, now starting to solve problem");

    let solution = problem.solve().unwrap();

    tracing::trace!("Solved problem!");

    let mut params = HashMap::new();

    for ((participant_id, room, role), variable) in x_irj.iter() {
        let value = solution.value(*variable);
        // might have rounding error
        if value >= 0.95 {
            match params.get_mut(*participant_id) {
                Some(_) => {
                    panic!(
                        "Error in ILP formulation, as this solution is not valid!"
                    )
                }
                None => {
                    params.insert(
                        **participant_id,
                        match role {
                            0 => Assignment::Team {
                                room: *room,
                                team: Team::Og,
                            },
                            1 => Assignment::Team {
                                room: *room,
                                team: Team::Oo,
                            },
                            2 => Assignment::Team {
                                room: *room,
                                team: Team::Cg,
                            },
                            3 => Assignment::Team {
                                room: *room,
                                team: Team::Co,
                            },
                            4 => Assignment::Judge(*room),
                            _ => unreachable!(),
                        },
                    );
                }
            }
        }
    }

    params
}

#[derive(
    Hash,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
)]
pub enum Team {
    Og,
    Oo,
    Cg,
    Co,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
/// An assignment of a [`db::spar::SparSeriesMember`] - the position in the
/// debate they have been allocated.
pub enum Assignment {
    /// Denotes that the person is allocated to the given room and team.
    Team { room: usize, team: Team },
    /// Denotes that the person is allocated as a judge in the given room.
    Judge(usize),
}

/// A room reconstructed from the solver output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolverRoom {
    /// The panel.
    pub panel: HashSet<i64>,
    pub teams: HashMap<
        Team,
        // maps each team (Og, Oo, Cg, Co) to the set of speakers
        HashSet<i64>,
    >,
}

#[tracing::instrument]
/// Constructs the rooms given the provided solution to the linear programming
/// problem.
pub fn rooms_of_speaker_assignments(
    params: &HashMap<i64, Assignment>,
) -> HashMap<usize, SolverRoom> {
    let mut rooms: HashMap<usize, SolverRoom> = HashMap::new();

    // fill in the rooms hashmap
    for (speaker_id, allocated_as) in params.iter() {
        match allocated_as {
            Assignment::Team { room, team } => {
                rooms
                    .entry(*room)
                    .and_modify(|room| {
                        room.teams
                            .entry(*team)
                            .and_modify(|t| {
                                t.insert(*speaker_id);
                            })
                            .or_insert({
                                let mut t = HashSet::new();
                                t.insert(*speaker_id);
                                t
                            });
                    })
                    .or_insert(SolverRoom {
                        panel: HashSet::new(),
                        teams: {
                            let mut t = HashMap::new();
                            t.insert(*team, {
                                let mut x = HashSet::new();
                                x.insert(*speaker_id);
                                x
                            });
                            t
                        },
                    });
            }
            Assignment::Judge(room) => {
                rooms
                    .entry(*room)
                    .and_modify(|room| {
                        room.panel.insert(*speaker_id);
                    })
                    .or_insert({
                        SolverRoom {
                            panel: {
                                let mut t = HashSet::with_capacity(3);
                                t.insert(*speaker_id);
                                t
                            },
                            teams: Default::default(),
                        }
                    });
            }
        }
    }

    rooms
}

pub fn team_of_int(int: usize) -> Team {
    match int {
        0 => Team::Og,
        1 => Team::Oo,
        2 => Team::Cg,
        3 => Team::Co,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod test_allocations {
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };

    use db::spar::SparSignup;
    use itertools::Itertools;

    use crate::spar_generation::allocation_problem::solve_allocation::solve_lp;

    use super::{rooms_of_speaker_assignments, Assignment};

    /// Generates a dummy dataset, useful for testing that the program generates
    /// the expected output.
    ///
    /// Note: Members are numbered from 0, with judges first, followed by
    /// speakers, and then followeed by those who have signed up to both judge
    /// and speak. The IDs are generated consecutively.
    fn generate_participants(
        judges: usize,
        speakers: usize,
        both: usize,
    ) -> HashMap<i64, SparSignup> {
        let mut ret = HashMap::new();
        let mut member_id = 0;
        let mut signup_id = 0;
        let mut incr_member_id = || -> i64 {
            let t = member_id;
            member_id += 1;
            t
        };
        let mut incr_signup_id = || -> i64 {
            let t = signup_id;
            signup_id += 1;
            t
        };

        for _ in 0..judges {
            let id = incr_member_id();

            ret.insert(
                id,
                SparSignup {
                    id: incr_signup_id(),
                    public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98"
                        .to_string(),
                    member_id: id,
                    spar_id: 0,
                    as_judge: true,
                    as_speaker: false,
                    partner_preference: None,
                },
            );
        }

        for _ in 0..speakers {
            let member_id = incr_member_id();
            let signup_id = incr_signup_id();
            ret.insert(
                member_id,
                SparSignup {
                    id: signup_id,
                    public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98"
                        .to_string(),
                    member_id,
                    spar_id: 0,
                    as_judge: false,
                    as_speaker: true,
                    partner_preference: None,
                },
            );
        }

        for _ in 0..both {
            let member_id = incr_member_id();
            let signup_id = incr_signup_id();
            ret.insert(
                member_id,
                SparSignup {
                    id: signup_id,
                    public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98"
                        .to_string(),
                    member_id,
                    spar_id: 0,
                    as_judge: true,
                    as_speaker: true,
                    partner_preference: None,
                },
            );
        }

        ret
    }

    #[test]
    fn three_rooms_three_judges() {
        let participants = Arc::new(generate_participants(3, 24, 0));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_speaker_assignments(&opt).len(), 3);
    }

    #[test]
    fn two_rooms_mix() {
        let participants = Arc::new(generate_participants(1, 16, 3));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = rooms_of_speaker_assignments(&opt);
        assert_eq!(rooms.len(), 2, "error: {rooms:#?} \n opt: {opt:?}");
    }

    #[test]
    fn judge_balance() {
        let participants = Arc::new(generate_participants(6, 16, 0));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = rooms_of_speaker_assignments(&opt);
        assert_eq!(rooms.len(), 2, "error: {rooms:#?} \n opt: {opt:?}");
        let mut iter = rooms.iter();
        let room1 = iter.next().unwrap().1;
        assert_eq!(room1.panel.len(), 3);
        let room2 = iter.next().unwrap().1;
        assert_eq!(room2.panel.len(), 3);
    }

    #[test]
    fn minimal_judges() {
        let participants = Arc::new(generate_participants(0, 12, 6));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = rooms_of_speaker_assignments(&opt);
        assert_eq!(rooms.len(), 2, "error: {rooms:#?} \n opt: {opt:?}");
        let mut iter = rooms.iter();
        let room1 = iter.next().unwrap().1;
        assert_eq!(room1.panel.len(), 1);
        let room2 = iter.next().unwrap().1;
        assert_eq!(room2.panel.len(), 1);
    }

    #[test]
    fn dczh_regression() {
        let participants = Arc::new(generate_participants(1, 15, 8));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = dbg!(rooms_of_speaker_assignments(&opt));
        assert_eq!(rooms.len(), 3, "error: {rooms:#?} \n opt: {opt:?}");

        let mut one_person_panel = 0;
        let mut two_person_panel = 0;

        let mut iter = rooms.iter();
        let room1 = iter.next().unwrap().1;
        match room1.panel.len() {
            1 => one_person_panel += 1,
            2 => two_person_panel += 1,
            _ => unreachable!(),
        }
        let room2 = iter.next().unwrap().1;
        match room2.panel.len() {
            1 => one_person_panel += 1,
            2 => two_person_panel += 1,
            _ => unreachable!(),
        }
        let room3 = iter.next().unwrap().1;
        match room3.panel.len() {
            1 => one_person_panel += 1,
            2 => two_person_panel += 1,
            _ => unreachable!(),
        }
        assert!(one_person_panel == 2 && two_person_panel == 1);
    }

    #[test]
    fn one_room_full() {
        let participants = Arc::new(generate_participants(0, 8, 1));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_speaker_assignments(&opt).len(), 1);
    }

    #[test]
    fn one_room_less_than_8() {
        let participants = Arc::new(generate_participants(0, 7, 1));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_speaker_assignments(&opt).len(), 1);
    }

    #[test]
    fn lots_of_rooms() {
        let n_rooms = 20;
        let participants =
            Arc::new(generate_participants(n_rooms * 3, n_rooms * 8, 0));
        let elo_scores = participants
            .iter()
            .map(|(member_id, _signup)| (*member_id, 25.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = rooms_of_speaker_assignments(&opt);

        assert_eq!(rooms.len(), n_rooms, "error: {rooms:?}, opt: {opt:?}");
    }

    #[test]
    fn pro_am_pairings() {
        let participants = Arc::new(generate_participants(1 * 3, 1 * 8, 0));
        let elo_scores = participants
            .iter()
            .sorted_by_key(|(id, _)| *id)
            .map(|(member_id, _signup)| {
                (*member_id, 25.0 * (*member_id as f64))
            })
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        let rooms = rooms_of_speaker_assignments(&opt);

        assert_eq!(rooms.len(), 1);
        let weakest_plus_strongest = {
            let mut t = HashSet::new();
            t.insert(3);
            t.insert(10);
            t
        };
        let room = rooms.values().next().unwrap().clone();
        let matched_weakest_and_strongest = room.teams.values().any(|team| {
            team.intersection(&weakest_plus_strongest)
                .collect::<HashSet<_>>()
                .len()
                == 2
        });
        assert!(matched_weakest_and_strongest, "failed: room {room:#?}")
    }

    fn assert_solution_valid(opt: HashMap<i64, Assignment>) {
        // we first generate a hashset of live rooms
        let rooms = {
            let mut rooms = HashSet::new();

            for (_speaker_id, param) in &opt {
                match param {
                    super::Assignment::Team { room, team: _ } => {
                        rooms.insert(room)
                    }
                    super::Assignment::Judge(room) => rooms.insert(room),
                };
            }

            rooms
        };

        for room in rooms {
            let judges = opt
                .iter()
                .filter_map(|(member_id, assignment)| match assignment {
                    super::Assignment::Team { room: _, team: _ } => None,
                    super::Assignment::Judge(_room) => Some(member_id),
                })
                .collect::<HashSet<_>>();
            assert!(
                judges.len() > 0,
                "error: judges too short! note: room={room}"
            );

            let teams = opt
                .iter()
                .filter_map(|(i, param)| match param {
                    super::Assignment::Team { room: r, team } if room == r => {
                        Some((i, team))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            let mut og = HashSet::new();
            let mut oo = HashSet::new();
            let mut cg = HashSet::new();
            let mut co = HashSet::new();

            for (speaker_idx, team) in teams {
                match team {
                    crate::spar_generation::allocation_problem::solve_allocation::Team::Og => {
                        og.insert(speaker_idx)
                    }
                    crate::spar_generation::allocation_problem::solve_allocation::Team::Oo => {
                        oo.insert(speaker_idx)
                    }
                    crate::spar_generation::allocation_problem::solve_allocation::Team::Cg => {
                        cg.insert(speaker_idx)
                    }
                    crate::spar_generation::allocation_problem::solve_allocation::Team::Co => {
                        co.insert(speaker_idx)
                    }
                };
            }

            assert_eq!(og.intersection(&judges).next(), None);
            assert_eq!(oo.intersection(&judges).next(), None);
            assert_eq!(cg.intersection(&judges).next(), None);
            assert_eq!(co.intersection(&judges).next(), None);

            assert_eq!(og.intersection(&oo).next(), None);
            assert_eq!(og.intersection(&cg).next(), None);
            assert_eq!(og.intersection(&co).next(), None);
            assert_eq!(oo.intersection(&cg).next(), None);
            assert_eq!(oo.intersection(&co).next(), None);
            assert_eq!(cg.intersection(&co).next(), None);

            assert!(
                1 <= og.len() && og.len() <= 2,
                "should have 1-2 speakers for og, instead have {}, room={room}",
                og.len()
            );
            assert!(
                1 <= oo.len() && oo.len() <= 2,
                "should have 1-2 speakers for oo, instead have {}, room={room}",
                oo.len()
            );
            assert!(
                1 <= cg.len() && cg.len() <= 2,
                "should have 1-2 speakers for cg, instead have {}, room={room}",
                cg.len()
            );
            assert!(
                1 <= co.len() && co.len() <= 2,
                "should have 1-2 speakers for co, instead have {}, room={room}",
                co.len()
            );
        }
    }
}
