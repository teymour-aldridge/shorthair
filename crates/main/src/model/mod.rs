//! A very simple simulator, intended to provide a robust guarantee of
//! correctness. This copies the approach outlined in
//! https://concerningquality.com/model-based-testing/

pub use model::{Action, State};
use uuid::Uuid;

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
}

mod model;

#[coverage(off)]
pub fn make_test_runner() -> impl Fn(&Vec<Action>) {
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

    let rocket = crate::make_rocket(&db_name.clone());

    let state = Arc::new(parking_lot::Mutex::new(State::of_rocket(rocket)));

    return move |actions: &Vec<Action>| {
        let mut conn = diesel::SqliteConnection::establish(&db_name.clone())
            .expect("Database connection failed");
        let mut state = state.lock();
        state.client.get("/logout").dispatch();
        state.reset();
        State::reset_db(&mut conn).unwrap();
        // necessary because login sessions are not stored in the database
        state.run(&actions, &mut conn);
    };
}

#[test]
pub fn do_model_test() {
    #[allow(unexpected_cfgs)]
    if cfg!(fuzzing) {
        use fuzzcheck::SerdeSerializer;
        use fuzzcheck::{mutators::vector::VecMutator, DefaultMutator};

        let result = fuzzcheck::fuzz_test(make_test_runner())
            .mutator(VecMutator::new(Action::default_mutator(), 0..=1000))
            .serializer(SerdeSerializer::default())
            .default_sensor_and_pool()
            .arguments_from_cargo_fuzzcheck()
            .launch();
        assert!(!result.found_test_failure);
    }
}

#[cfg(test)]
/// The naming scheme used for these is a bit strange.
mod regressions {
    use super::make_test_runner;

    #[test]
    fn regressions() {
        let runner = make_test_runner();
        // (runner)(
        //     &serde_json::from_str(include_str!(
        //         "testcases/9aca06d881c8f7d8.json"
        //     ))
        //     .unwrap(),
        // );
        // println!("finished regression 1");
        // (runner)(
        //     &serde_json::from_str(include_str!(
        //         "testcases/af03e2dfc3f943a9.json"
        //     ))
        //     .unwrap(),
        // );
        // println!("finished regression 2");
        // (runner)(
        //     &serde_json::from_str(include_str!(
        //         "testcases/647fca9e700fa63b.json"
        //     ))
        //     .unwrap(),
        // );
        // println!("finished regression 3");
        (runner)(
            &serde_json::from_str(include_str!(
                "testcases/cba213fd9f4e7bae.json"
            ))
            .unwrap(),
        );
    }
}
