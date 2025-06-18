//! A very simple simulator, intended to provide a robust guarantee of
//! correctness. This copies the approach outlined in
//! https://concerningquality.com/model-based-testing/

pub use model::{Action, State};
use uuid::Uuid;

#[test]
pub fn do_model_test() {
    #[allow(unexpected_cfgs)]
    if cfg!(fuzzing) {
        use fuzzcheck::SerdeSerializer;
        use fuzzcheck::{mutators::vector::VecMutator, DefaultMutator};

        let result = fuzzcheck::fuzz_test(make_test_runner())
            .mutator(VecMutator::new(Action::default_mutator(), 0..=50))
            .serializer(SerdeSerializer::default())
            .default_sensor_and_pool()
            .arguments_from_cargo_fuzzcheck()
            .launch();
        assert!(!result.found_test_failure);
    }
}

pub mod sync {
    pub mod id {
        use std::sync::Mutex;

        use uuid::Uuid;

        lazy_static::lazy_static! {
            static ref PREV_IDS: Mutex<Vec<Uuid>> = Mutex::new(vec![]);
        }

        /// Generate a new UUID.
        pub fn gen_uuid() -> Uuid {
            let id = Uuid::new_v4();

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

    let db_name = Arc::new(format!("{}.db", Uuid::new_v4()));

    let mut conn = diesel::SqliteConnection::establish(&db_name.to_string())
        .expect("Database connection failed");
    diesel::sql_query("PRAGMA journal_mode=WAL")
        .execute(&mut conn)
        .expect("Failed to enable WAL mode");
    diesel::sql_query("PRAGMA busy_timeout = 1000;")
        .execute(&mut conn)
        .expect("Failed to set busy timeout");
    diesel::sql_query("PRAGMA foreign_keys = ON;")
        .execute(&mut conn)
        .expect("Failed to enable foreign keys");

    let rocket = crate::make_rocket(&db_name.clone());
    let figment = rocket.figment().clone().merge((
        "secret_key",
        "7db4970440b3092a69247a841bd8c566c514e4ded9d4952ce7febf3381110a24",
    ));
    let rocket = rocket.configure(figment);

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

#[cfg(test)]
/// The naming scheme used for these is a bit strange.
mod regressions {
    use super::make_test_runner;

    #[test]
    fn regression1() {
        let runner = make_test_runner();
        let actions = serde_json::from_str(
            r#"[
          {
            "Setup": {
              "id": 2730416644147746095,
              "public_id": "",
              "username": "uPzl",
              "email": "user@example.com",
              "email_verified": true,
              "password_hash": "Person Lastname7",
              "created_at": "2035-04-06T08:56:32",
              "is_superuser": true,
              "may_create_resources": true
            }
          },
          { "Login": 4 },
          {
            "CreateGroup": {
              "id": -1883042837273410931,
              "public_id": "\u0015�",
              "name": "vhJ-O:R `cCW >Rv/*WW0jGptLQ\\z",
              "website": "d�=��uN�",
              "created_at": "1983-01-15T12:30:19"
            }
          },
          {
            "CreateSparSeries": {
              "id": 3362060881343125898,
              "public_id": "\u0007\u0007m\u0003\u0006���/3�Վ\fԑmQt",
              "title": "user@example.com",
              "description": null,
              "speakers_per_team": -121205089288800765,
              "group_id": 5682687000259006193,
              "created_at": "1922-09-05T00:34:07",
              "allow_join_requests": false,
              "auto_approve_join_requests": true
            }
          },
          {
            "AddMember": {
              "id": 5643998041964162977,
              "public_id": "��",
              "name": "42w?Co/'>wS{N;MOV{KkF?$d+(>%Hn 2Wne0ZtV,g{Z/.x",
              "email": "jIG8>:Qr+j@tjFO,KHRU4l^7u!",
              "spar_series_id": 7817833345152907556,
              "created_at": "1977-01-09T00:34:55"
            }
          }
        ]"#,
        )
        .unwrap();
        (runner)(&actions)
    }

    #[test]
    fn regression2() {
        let runner = make_test_runner();
        let actions = serde_json::from_str(
            r#"
            [
              {
                "Setup": {
                  "id": -8721541264781336897,
                  "public_id": "Q\u0001�i;6=�\"�3溻�t\b�\u0012��xS��SAZ\u0014p>/\u0013\u0017\u0007\u001d��\t�m\u0003r�\u0013��\u0005�",
                  "username": "qUcIuf",
                  "email": "user@example.com",
                  "email_verified": false,
                  "password_hash": "Person Lastname3",
                  "created_at": "1974-07-22T10:28:52",
                  "is_superuser": true,
                  "may_create_resources": false
                }
              },
              { "Login": 4 },
              {
                "CreateGroup": {
                  "id": -6754041804169275553,
                  "public_id": "�H��",
                  "name": "person77",
                  "website": "+\u001c�ː|�",
                  "created_at": "1905-10-22T01:06:56"
                }
              },
              {
                "CreateSparSeries": {
                  "id": 8456802816100193075,
                  "public_id": "\u001c=\u0018�_�",
                  "title": "person08",
                  "description": "user@example.com",
                  "speakers_per_team": 3563706608828805121,
                  "group_id": 7189249649865157095,
                  "created_at": "2016-10-27T01:31:51",
                  "allow_join_requests": true,
                  "auto_approve_join_requests": false
                }
              },
              {
                "CreateSpar": {
                  "id": -6552207435512487021,
                  "public_id": "",
                  "start_time": "1947-07-27T05:42:39",
                  "is_open": false,
                  "release_draw": false,
                  "spar_series_id": 4913609151390805753,
                  "is_complete": false,
                  "created_at": "1913-11-28T03:32:46"
                }
              },
              {
                "CreateSparSeries": {
                  "id": 8456802816100193075,
                  "public_id": "\u001c=\u0018�_�",
                  "title": "person08",
                  "description": "user@example.com",
                  "speakers_per_team": 3563706608828805121,
                  "group_id": 7189249649865157095,
                  "created_at": "2016-10-27T01:31:51",
                  "allow_join_requests": true,
                  "auto_approve_join_requests": false
                }
              },
              {
                "CreateSpar": {
                  "id": -6552207435512487021,
                  "public_id": "",
                  "start_time": "1947-07-27T05:42:39",
                  "is_open": false,
                  "release_draw": false,
                  "spar_series_id": 4913609151390805753,
                  "is_complete": false,
                  "created_at": "1913-11-28T03:32:46"
                }
              }
            ]
            "#,
        )
        .unwrap();
        (runner)(&actions)
    }

    #[test]
    fn regression3() {
        let runner = make_test_runner();
        let actions = serde_json::from_str(
            r#"
            [
              {
                "Setup": {
                  "id": -8721541264781336897,
                  "public_id": "",
                  "username": "UcIuf",
                  "email": "r@ple.com",
                  "email_verified": false,
                  "password_hash": "person0",
                  "created_at": "1974-07-22T10:28:52",
                  "is_superuser": true,
                  "may_create_resources": false
                }
              },
              { "Login": 4 },
              {
                "CreateGroup": {
                  "id": -6754041804169275553,
                  "public_id": "",
                  "name": "I@K.CX",
                  "website": "+\u001cː|",
                  "created_at": "1905-10-22T01:06:56"
                }
              },
              {
                "CreateGroup": {
                  "id": -6754041804169275553,
                  "public_id": "",
                  "name": "Iuf",
                  "website": "+\u001cː|",
                  "created_at": "1905-10-22T01:06:56"
                }
              }
            ]
            "#,
        )
        .unwrap();
        (runner)(&actions)
    }

    #[test]
    fn regression4() {
        let runner = make_test_runner();
        let actions = serde_json::from_str(
            r#"
            [
              {
                "Setup": {
                  "id": -8721541264781336897,
                  "public_id": "Iuf",
                  "username": "qUcIuf",
                  "email": "user@example.com",
                  "email_verified": false,
                  "password_hash": "person21",
                  "created_at": "1974-07-22T10:28:52",
                  "is_superuser": true,
                  "may_create_resources": false
                }
              },
              { "Login": 4 },
              {
                "CreateGroup": {
                  "id": -6754041804169275553,
                  "public_id": "H",
                  "name": "person77",
                  "website": "+\u001cː|",
                  "created_at": "1905-10-22T01:06:56"
                }
              },
              {
                "CreateSparSeries": {
                  "id": 8456802816100193075,
                  "public_id": "\u001c=\u0018_",
                  "title": "person08",
                  "description": null,
                  "speakers_per_team": 3563706608828805121,
                  "group_id": 7189249649865157095,
                  "created_at": "2016-10-27T01:31:51",
                  "allow_join_requests": true,
                  "auto_approve_join_requests": false
                }
              },
              {
                "CreateSpar": {
                  "id": -8354458756482984002,
                  "public_id": "",
                  "start_time": "1947-07-27T05:42:39",
                  "is_open": false,
                  "release_draw": false,
                  "spar_series_id": 4913609151390805753,
                  "is_complete": false,
                  "created_at": "1913-11-28T03:32:46"
                }
              },
              {
                "CreateSparSeries": {
                  "id": 4231715758590899287,
                  "public_id": "m~N{\"z",
                  "title": "Person Lastname7660",
                  "description": null,
                  "speakers_per_team": -4108937861228846482,
                  "group_id": 8006756651933283363,
                  "created_at": "1958-05-01T23:18:07",
                  "allow_join_requests": true,
                  "auto_approve_join_requests": true
                }
              },
              {
                "CreateSpar": {
                  "id": -8354458756482984002,
                  "public_id": "",
                  "start_time": "1947-07-27T05:42:39",
                  "is_open": false,
                  "release_draw": false,
                  "spar_series_id": 4913609151390805753,
                  "is_complete": false,
                  "created_at": "1913-11-28T03:32:46"
                }
              }
            ]
            "#,
        )
        .unwrap();
        (runner)(&actions)
    }
}
