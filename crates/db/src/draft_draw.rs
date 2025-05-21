use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use chumsky::{prelude::just, text::whitespace, Parser};
use diesel::prelude::Queryable;
use fuzzcheck::DefaultMutator;
use fuzzcheck_util::chrono_mutators::{
    naive_date_time_mutator, NaiveDateTimeMutator,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

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

#[derive(
    Debug,
    Queryable,
    Serialize,
    Deserialize,
    Clone,
    Hash,
    PartialEq,
    Eq,
    DefaultMutator,
)]
/// A draw which has not yet been confirmed.
pub struct DraftDraw {
    pub id: i64,
    pub public_id: String,
    pub data: Option<String>,
    pub spar_id: i64,
    pub version: i64,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct DraftDrawData {
    pub rooms: Vec<DraftDrawRoom>,
    /// Maps individual participants to unique short (e.g. two-letter ids)
    pub id_map: HashMap<i64, u64>,
}

#[derive(Copy, Clone, Debug)]
pub enum EditAction {
    Swap(u64, u64),
    Remove(u64),
}

fn edit_action_parser<'src>() -> impl Parser<'src, &'src str, EditAction> {
    (just("swap")
        .then_ignore(whitespace())
        .ignore_then(chumsky::prelude::text::int(10))
        .then_ignore(whitespace())
        .then(chumsky::prelude::text::int(10)))
    .map(|(a, b): (&str, &str)| {
        EditAction::Swap(a.parse().unwrap(), b.parse().unwrap())
    })
    .or(just("remove")
        .then_ignore(whitespace())
        .ignore_then(chumsky::prelude::text::int(10))
        .map(|a: &str| EditAction::Remove(a.parse().unwrap())))
}

impl EditAction {
    pub fn parse(cmd: &str) -> Result<EditAction, EditError> {
        let parser_output = edit_action_parser().parse(cmd);

        if parser_output.has_errors() {
            let err = parser_output.errors().map(|e| e.to_string()).join("\n");
            Err(EditError::ParseErr(err))
        } else {
            Ok(*parser_output.output().unwrap())
        }
    }
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum DrawLoc {
    Panel,
    Team(Team),
}

#[derive(Debug)]
pub enum EditError {
    NoPersonWithId,
    ParseErr(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoPersonWithId => write!(f, "invalid ID provided"),
            EditError::ParseErr(err) => write!(f, "parse error: {err}"),
        }
    }
}

impl DraftDrawData {
    pub fn generate_map(&mut self) {
        let mut id = 0;
        for room in &self.rooms {
            for judge in &room.panel {
                self.id_map.insert(*judge, id);
                id += 1;
            }
            for team in room.teams.values() {
                for speaker in team {
                    self.id_map.insert(*speaker, id);
                    id += 1;
                }
            }
        }
    }

    pub fn lookup_idx(&self, idx: u64) -> Result<i64, EditError> {
        self.id_map
            .iter()
            .find(|(_, v)| **v == idx)
            .map(|(k, _)| *k)
            .ok_or(EditError::NoPersonWithId)
    }

    /// Apply the given action to the current draw. This creates a new version
    /// of the current draw.
    ///
    /// We only retain up to 10 steps of history. After draws are published, we
    /// delete all but the latest version.
    #[tracing::instrument(skip(self))]
    pub fn apply(
        &self,
        action: EditAction,
    ) -> Result<DraftDrawData, EditError> {
        let mut new_draw = self.clone();
        match action {
            EditAction::Swap(a, b) => {
                tracing::trace!("Swapping {a} and {b}");
                if a != b {
                    let a = self.lookup_idx(a)?;
                    let b = self.lookup_idx(b)?;

                    let (a_idx, a_loc) = new_draw.find_loc(a);
                    let (b_idx, b_loc) = new_draw.find_loc(b);

                    if (a_loc != b_loc) || (a_idx != b_idx) {
                        let a_set = new_draw
                            .get_team_or_panel_set_mut(a_idx, a_loc)
                            .unwrap();

                        a_set.remove(&a);
                        a_set.insert(b);

                        let b_set = new_draw
                            .get_team_or_panel_set_mut(b_idx, b_loc)
                            .unwrap();

                        b_set.remove(&b);
                        b_set.insert(a);
                    }
                }
            }
            EditAction::Remove(member) => {
                let member = self.lookup_idx(member)?;
                for room in &mut new_draw.rooms {
                    room.panel.remove(&member);
                    for (_, team) in room.teams.iter_mut() {
                        team.remove(&member);
                    }
                }
            }
        };
        Ok(new_draw)
    }

    fn get_team_or_panel_set_mut(
        &mut self,
        room: usize,
        loc: DrawLoc,
    ) -> Option<&mut HashSet<i64>> {
        match self.rooms.get_mut(room) {
            Some(room) => match loc {
                DrawLoc::Panel => Some(&mut room.panel),
                DrawLoc::Team(team) => Some(
                    room.teams
                        .get_mut(&team)
                        .expect("that all rooms have an entry for each team"),
                ),
            },
            None => None,
        }
    }

    /// Find the location (room + team/panel) of the given person.
    ///
    /// This function should only be called with valid member ids (it will
    /// panic otherwise).
    fn find_loc(&self, a: i64) -> (usize, DrawLoc) {
        for (idx, room) in self.rooms.iter().enumerate() {
            if room.panel.contains(&a) {
                return (idx, DrawLoc::Panel);
            }

            for (team, members) in room.teams.iter() {
                if members.contains(&a) {
                    return (idx, DrawLoc::Team(*team));
                }
            }
        }

        unreachable!()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct DraftDrawRoom {
    pub panel: HashSet<i64>,
    pub teams: HashMap<
        Team,
        // maps each team (Og, Oo, Cg, Co) to the set of speakers
        HashSet<i64>,
    >,
}

#[cfg(test)]
pub mod test_draft_draw {
    use std::collections::{HashMap, HashSet};

    use super::{DraftDrawData, DraftDrawRoom, EditAction, Team};

    #[test]
    fn simple() {
        let data = DraftDrawData {
            rooms: vec![DraftDrawRoom {
                panel: {
                    let mut set = HashSet::new();
                    set.insert(0);
                    set.insert(1);
                    set
                },
                teams: {
                    let mut map = HashMap::new();
                    map.insert(Team::Og, {
                        let mut set = HashSet::new();
                        set.insert(2);
                        set.insert(3);
                        set
                    });
                    map.insert(Team::Oo, {
                        let mut set = HashSet::new();
                        set.insert(4);
                        set.insert(5);
                        set
                    });
                    map.insert(Team::Cg, {
                        let mut set = HashSet::new();
                        set.insert(6);
                        set.insert(7);
                        set
                    });
                    map.insert(Team::Co, {
                        let mut set = HashSet::new();
                        set.insert(8);
                        set
                    });
                    map
                },
            }],
            id_map: {
                let mut map = HashMap::new();
                for i in 0..=8 {
                    map.insert(i as i64, i as u64);
                }
                map
            },
        };

        // Original test case: swap 8 and 6
        let new_draw = data.apply(EditAction::Swap(8, 6)).unwrap();

        let cg = new_draw.rooms[0].teams.get(&Team::Cg).unwrap();
        let co = new_draw.rooms[0].teams.get(&Team::Co).unwrap();

        assert!(
            cg.contains(&8) && !cg.contains(&6),
            "cg = {cg:?}, co = {co:?}"
        );
        assert!(co.len() == 1 && co.contains(&6), "cg = {cg:?}, co = {co:?}");

        let same = data.apply(EditAction::Swap(6, 6)).unwrap();
        assert_eq!(same, data);
    }
}
