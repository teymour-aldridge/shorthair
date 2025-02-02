//! Generates a draw by formulating the problem as an ILP, and then using a
//! solver to resolve this.
//!
//! In our problem, we have variables
//!     x_{i, r, j} := is person i assigned to role j in room r
//!     r           := a room in {1, ..., R_max} where R_{max} is the maximum
//!                    number of rooms we might need
//!     u_r         := denotes whether room r is used or not
//!
//! All solutions need to satisfy the constraints (todo: update this - I fixed
//! some problems with the constraints during the implementation)
//!     x_{i, r, j}               <= u_r for all r
//!     sum_{i} [x_{i, j, r}]     <= 2 * u_r for all rooms r
//!                                          and all team roles j
//!     sum_{i} [x_{i, JUDGE, r}] >= u_r     for all rooms r
//!     sum_{i} [x_{i, JUDGE, r}] <= n * u_r for all rooms r
//!     u_r <= sum_{i} [x_{i, JUDGE, r}]
//!
//!
//! Then we seek to reduce the number of rooms we use
//! - sum_{r} u_r
//!
//! as well as the ELO difference between rooms

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use db::spar::SparSignup;
use good_lp::{
    constraint, variables, Expression, Solution, SolverModel,
    VariableDefinition,
};

/// Always remember: if it runs in polynomial time, it's efficient (for
/// constructing the problem instance).
pub fn solve_lp(
    participants: Arc<Vec<SparSignup>>,
    elo_scores: HashMap<usize, f64>,
) -> Vec<Param> {
    // maximum number of rooms
    let r_max = participants
        .iter()
        .filter(|signup| signup.as_speaker)
        .count();

    let mut vars = variables!();

    // create the variables
    //
    // the names are as in the descriptions above
    let (x_irj, u_r) = {
        let mut x_irj = HashMap::new();
        let mut u_r = HashMap::new();
        for (participant_idx, _) in participants.iter().enumerate() {
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
                            format!("x({participant_idx}, {room_idx}, {role})"),
                        ));
                    x_irj.insert((participant_idx, room_idx, role), variable);
                }
            }
        }

        (x_irj, u_r)
    };

    let mut constraints = Vec::new();

    let () = {
        for participant_idx in 0..participants.len() {
            let record = &participants[participant_idx];

            // add judge/speaker constraints
            for room in 0..r_max {
                // if not signed up as a judge, then we add a constraint which
                // prevents the user from being allocated as a judge
                assert!(record.as_judge || record.as_speaker);
                if !record.as_judge {
                    let constraint = constraint! {
                        x_irj[&(participant_idx, room, 4)] <= 0
                    };
                    constraints.push(constraint);
                }

                if !record.as_speaker {
                    for role in 0..4usize {
                        let constraint = constraint! {
                            x_irj[&(participant_idx, room, role)] <= 0
                        };
                        constraints.push(constraint);
                    }
                }
            }

            // add constraint requiring each user to be allocated in exactly one
            // position (we set sum [positions allocated] >= 1
            //                  AND sum [positions_allocated] <= 1)
            let mut positions_allocated = Expression::default();
            for room in 0..r_max {
                for role in 0..5usize {
                    positions_allocated +=
                        x_irj[&(participant_idx, room, role)];
                }
            }
            constraints.push(constraint! {
                positions_allocated.clone() >= 1
            });
            constraints.push(constraint! {
                positions_allocated <= 1
            })
        }
    };

    let () = {
        for room in 0..r_max {
            let mut judge_count = Expression::default();
            let mut og_count = Expression::default();
            let mut oo_count = Expression::default();
            let mut cg_count = Expression::default();
            let mut co_count = Expression::default();

            for participant_idx in 0..participants.len() {
                judge_count += x_irj[&(participant_idx, room, 4)];
                og_count += x_irj[&(participant_idx, room, 0)];
                oo_count += x_irj[&(participant_idx, room, 1)];
                cg_count += x_irj[&(participant_idx, room, 2)];
                co_count += x_irj[&(participant_idx, room, 3)];
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
        }
    };

    // minimise difference between teams
    let difference_between_teams_objective = {
        // ELO score of each team
        //
        // this is in the form (room_idx, role)
        let mut score_per_team = HashMap::new();

        let mut difference_between_teams_objective = Expression::default();
        for room_idx in 0..r_max {
            for role in 0..4 {
                // efficiency... what does this word "efficiency" mean?
                let variables_for_this_role_and_room = x_irj
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
                    .map(|((i, _j, _r), v)| {
                        let score: Expression =
                            *v * *elo_scores.get(i).unwrap_or(&1500.0);
                        score
                    })
                    .collect::<Vec<_>>();

                score_per_team.insert(
                    (room_idx, role),
                    variables_for_this_role_and_room
                        .iter()
                        .map(|exp| {
                            exp.clone()
                                / (variables_for_this_role_and_room.len()
                                    as f64)
                        })
                        .sum::<Expression>(),
                );
            }

            for r1 in 0..4 {
                for r2 in (r1 + 1)..4 {
                    let team_1 = score_per_team[&(room_idx, r1)].clone();
                    let team_2 = score_per_team[&(room_idx, r2)].clone();

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
                    difference_between_teams_objective +=
                        absolute_value_of_difference;
                }
            }
        }
        difference_between_teams_objective
    };

    // todo: maximise the difference between speakers on a team
    let difference_between_speakers_objective = { 0.0 };

    // we want fewer rooms (where possible)
    let fewer_rooms_objective = {
        let mut room_count = Expression::default();

        for i in 0..r_max {
            room_count += u_r[&i];
        }

        room_count
    };

    // todo: difference between speakers on the same team

    let mut problem = vars
        .minimise(
            difference_between_teams_objective
                + /* todo: multiplier here */ difference_between_speakers_objective
                + /* todo: multiplier here */ fewer_rooms_objective,
        )
        .using(good_lp::solvers::highs::highs);

    // add constraints to problem
    for constraint in constraints {
        problem = problem.with(constraint);
    }

    let solution = problem
        .set_solver(good_lp::solvers::highs::HighsSolverType::Choose)
        .solve()
        .unwrap();

    let mut params = vec![None; participants.len()];

    for ((participant_idx, room, role), variable) in x_irj.iter() {
        let value = solution.value(*variable);
        // might have rounding error
        if value >= 0.95 {
            match params[*participant_idx] {
                Some(_) => panic!(
                    "Error in ILP formulation, as this solution is not valid!"
                ),
                None => {
                    *params.get_mut(*participant_idx).unwrap() =
                        Some(match role {
                            0 => Param::Team {
                                room: *room,
                                team: Team::Og,
                            },
                            1 => Param::Team {
                                room: *room,
                                team: Team::Oo,
                            },
                            2 => Param::Team {
                                room: *room,
                                team: Team::Cg,
                            },
                            3 => Param::Team {
                                room: *room,
                                team: Team::Co,
                            },
                            4 => Param::Judge(*room),
                            _ => unreachable!(),
                        });
                }
            }
        }
    }

    params.into_iter().map(Option::unwrap).collect::<Vec<_>>()
}

#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum Team {
    Og,
    Oo,
    Cg,
    Co,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    // allocate to the given team in the given room
    Team { room: usize, team: Team },
    // put on a panel in the nth room
    Judge(usize),
}

/// A room reconstructed from the solver output.
#[derive(Debug, Clone)]
pub struct SolverRoom {
    /// The panel. Note that each item in this list provides an index into
    /// the list of
    pub panel: Vec<usize>,
    pub teams: HashMap<Team, HashSet<usize>>,
}

pub fn rooms_of_params(params: &[Param]) -> HashMap<usize, SolverRoom> {
    let mut rooms: HashMap<usize, SolverRoom> = HashMap::new();

    // fill in the rooms hashmap
    for (person_idx, allocated_as) in params.iter().enumerate() {
        match allocated_as {
            Param::Team { room, team } => {
                rooms
                    .entry(*room)
                    .and_modify(|room| {
                        room.teams
                            .entry(*team)
                            .and_modify(|t| {
                                t.insert(person_idx);
                            })
                            .or_insert({
                                let mut t = HashSet::new();
                                t.insert(person_idx);
                                t
                            });
                    })
                    .or_insert(SolverRoom {
                        panel: vec![],
                        teams: {
                            let mut t = HashMap::new();
                            t.insert(*team, {
                                let mut x = HashSet::new();
                                x.insert(person_idx);
                                x
                            });
                            t
                        },
                    });
            }
            Param::Judge(room) => {
                rooms
                    .entry(*room)
                    .and_modify(|room| {
                        room.panel.push(person_idx);
                    })
                    .or_insert({
                        SolverRoom {
                            panel: vec![person_idx],
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

    use crate::spar_allocation::solve_allocation::solve_lp;

    use super::{rooms_of_params, Param};

    fn generate_participants(
        judges: usize,
        speakers: usize,
        both: usize,
    ) -> Vec<SparSignup> {
        let mut ret = Vec::new();
        let mut c = 0;
        let mut incr_c = || {
            let t = c;
            c += 1;
            t
        };

        for _ in 0..judges {
            ret.push(SparSignup {
                id: incr_c(),
                public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98".to_string(),
                user_id: incr_c(),
                spar_id: 0,
                as_judge: true,
                as_speaker: false,
            })
        }

        for _ in 0..speakers {
            ret.push(SparSignup {
                id: incr_c(),
                public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98".to_string(),
                user_id: incr_c(),
                spar_id: 0,
                as_judge: false,
                as_speaker: true,
            })
        }

        for _ in 0..both {
            ret.push(SparSignup {
                id: incr_c(),
                public_id: "64abde2a-ed68-49da-a4d8-860ebefe6f98".to_string(),
                user_id: incr_c(),
                spar_id: 0,
                as_judge: true,
                as_speaker: true,
            })
        }

        ret
    }

    #[test]
    fn three_rooms_three_judges() {
        let participants = Arc::new(generate_participants(3, 24, 0));
        let elo_scores = participants
            .iter()
            .enumerate()
            .map(|(idx, _)| (idx, 1500.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_params(&opt).len(), 3);
    }

    #[test]
    fn two_rooms_mix() {
        let participants = Arc::new(generate_participants(1, 16, 3));
        let elo_scores = participants
            .iter()
            .enumerate()
            .map(|(idx, _)| (idx, 1500.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_params(&opt).len(), 2);
    }

    #[test]
    fn one_room_full() {
        let participants = Arc::new(generate_participants(0, 8, 1));
        let elo_scores = participants
            .iter()
            .enumerate()
            .map(|(idx, _)| (idx, 1500.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_params(&opt).len(), 1);
    }

    #[test]
    fn one_room_less_than_8() {
        let participants = Arc::new(generate_participants(0, 7, 1));
        let elo_scores = participants
            .iter()
            .enumerate()
            .map(|(idx, _)| (idx, 1500.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_params(&opt).len(), 1);
    }

    #[test]
    fn lots_of_rooms() {
        let participants = Arc::new(generate_participants(10 * 3, 10 * 8, 0));
        let elo_scores = participants
            .iter()
            .enumerate()
            .map(|(idx, _)| (idx, 1500.0))
            .collect::<HashMap<_, _>>();

        let opt = solve_lp(participants, elo_scores);

        assert_solution_valid(opt.clone());

        assert_eq!(rooms_of_params(&opt).len(), 10);
    }

    fn assert_solution_valid(opt: Vec<Param>) {
        let rooms = {
            let mut rooms = HashSet::new();

            for param in &opt {
                match param {
                    super::Param::Team { room, team: _ } => rooms.insert(room),
                    super::Param::Judge(room) => rooms.insert(room),
                };
            }

            rooms
        };

        for room in rooms {
            let judges = opt
                .iter()
                .filter(|param| match param {
                    super::Param::Team { room: _, team: _ } => false,
                    super::Param::Judge(_room) => true,
                })
                .collect::<Vec<_>>();
            assert!(
                judges.len() > 0,
                "error: judges too short! note: room={room}"
            );

            let teams = opt
                .iter()
                .enumerate()
                .filter_map(|(i, param)| match param {
                    super::Param::Team { room: r, team } if room == r => {
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
                    crate::spar_allocation::solve_allocation::Team::Og => {
                        og.insert(speaker_idx)
                    }
                    crate::spar_allocation::solve_allocation::Team::Oo => {
                        oo.insert(speaker_idx)
                    }
                    crate::spar_allocation::solve_allocation::Team::Cg => {
                        cg.insert(speaker_idx)
                    }
                    crate::spar_allocation::solve_allocation::Team::Co => {
                        co.insert(speaker_idx)
                    }
                };
            }

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
