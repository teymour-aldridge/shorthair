//! A very simple simulator, intended to provide a robust guarantee of
//! correctness. This copies the approach outlined in
//! https://concerningquality.com/model-based-testing/
//!
//! To generate sequences of actions, I use LLVM's libfuzzer.

pub mod sync {
    pub mod id {
        use std::sync::Mutex;

        use uuid::Uuid;

        lazy_static::lazy_static! {
            static ref PREV_IDS: Mutex<Vec<Uuid>> = Mutex::new(vec![]);
        }

        /// Generate a new UUID.
        pub fn gen_uuid() -> Uuid {
            let id = Uuid::now_v7();

            // we only store the ID when testing
            #[cfg(test)]
            {
                let mut prev = PREV_IDS.lock().unwrap();
                prev.push(id);
            }

            id
        }

        /// Returns the last generated UUID. This is necessary to synchronize state
        /// between the real application and the model.
        pub fn last_id() -> Option<Uuid> {
            let prev = PREV_IDS.lock().unwrap();
            prev.last().cloned()
        }

        /// Returns the nth most recent ID.
        ///
        /// That is
        /// f(0) = last id generated
        /// f(1) = id generated before that
        /// ...
        /// f(k) = id generated k + 1 steps ago
        pub fn nth_most_recent_id(n: usize) -> Option<Uuid> {
            let prev = PREV_IDS.lock().unwrap();
            prev.get(n).cloned()
        }
    }

    pub mod draw {
        //! Sync draws that are generated in the app to the model. We do not
        //! test the correctness of draws in the model. This is done through
        //! separate (manual) testing of the application.
        use std::{
            collections::{HashMap, HashSet},
            sync::{Arc, Mutex},
        };

        use db::spar::SparSignup;
        use uuid::Uuid;

        use crate::spar_allocation::solve_allocation::Team;

        lazy_static::lazy_static! {
            /// Maps room IDs to the relevant draws.
            ///
            /// todo: if we add support for multiple draws
            /// (draws are not deterministic) we should store all the previous
            /// copies here
            ///
            /// Note: putting all the participants in an [`Arc`] was entirely
            /// accidental, but works out quite nicely here.
            static ref DRAWS: Mutex<HashMap<Uuid, (Arc<Vec<SparSignup>>,Vec<SolverRoomWithIds>)>> = Mutex::new(HashMap::new());
        }

        #[derive(Debug, Clone)]
        pub struct UserRefId {
            pub my_id: Uuid,
            pub user_id: Uuid,
        }

        /// This is a very crude way to just pass all the IDs in correctly.
        #[derive(Debug, Clone)]
        pub struct SolverRoomWithIds {
            pub room_id: Uuid,
            pub panel: Vec<(usize, UserRefId)>,
            pub teams: HashMap<
                Team,
                (
                    HashSet<(
                        usize,
                        /* id of spar_room_team_speakers record */
                        Uuid,
                    )>,
                    /* id of spar_room_teams record */
                    Uuid,
                ),
            >,
        }

        pub fn get_draw(
            spar: Uuid,
        ) -> Option<(Arc<Vec<SparSignup>>, Vec<SolverRoomWithIds>)> {
            let draws = DRAWS.lock().unwrap();
            draws.get(&spar).cloned()
        }

        pub fn store_draw(
            _spar: Uuid,
            _participants: Arc<Vec<SparSignup>>,
            _draw: Vec<SolverRoomWithIds>,
        ) {
            #[cfg(test)]
            {
                let mut draws = DRAWS.lock().unwrap();
                draws.insert(_spar, (_participants, _draw));
            }
        }
    }
}

#[cfg(test)]
pub mod model;
