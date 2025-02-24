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
        state.reset();
        State::reset_db(&mut conn).unwrap();
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
    pub fn reg_1() {
        let runner = make_test_runner();
        (runner)(&serde_json::from_str("[]").unwrap())
    }

    #[test]
    pub fn reg_2() {
        let runner = make_test_runner();
        (runner)(&serde_json::from_str(r#"
            [{"Setup":{"id":-7488954413955720345,"public_id":"","username":"","email":"","email_verified":true,"password_hash":"","created_at":"1974-01-23T12:25:44","is_superuser":false}}]
            "#).unwrap())
    }

    #[test]
    pub fn reg_3() {
        let runner = make_test_runner();
        (runner)(&serde_json::from_str(r#"
            [{"Setup":{"id":2782220094895887879,"public_id":"","username":"","email":"","email_verified":true,"password_hash":"�\u001b�","created_at":"2021-10-30T09:00:43","is_superuser":false}}]
            "#).unwrap())
    }

    #[test]
    pub fn reg_4() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": 7456426718206932077,
                      "public_id": "",
                      "username": "personI",
                      "email": "user@example.com",
                      "email_verified": false,
                      "password_hash": "person7",
                      "created_at": "1959-11-16T12:09:11",
                      "is_superuser": true
                    }
                  },
                  { "Login": 22 },
                  {
                    "CreateGroup": {
                      "id": -1275130027378103291,
                      "public_id": "�",
                      "name": "person208",
                      "website": null,
                      "created_at": "2014-04-06T13:47:39"
                    }
                  },
                  {
                    "CreateSparSeries": {
                      "id": -7319772347446247088,
                      "public_id": "*y\u000f_S���F�@�f'�p",
                      "title": ")LmlKB",
                      "description": null,
                      "speakers_per_team": 8971086411835473157,
                      "group_id": 618065765806848772,
                      "created_at": "1931-10-06T09:32:12"
                    }
                  },
                  {
                    "CreateSpar": {
                      "id": -6150881746904209018,
                      "public_id": "",
                      "start_time": "2026-06-01T05:17:01",
                      "is_open": false,
                      "release_draw": false,
                      "spar_series_id": 1330336505694704986,
                      "created_at": "2003-04-12T10:45:19"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_5() {
        let runner = make_test_runner();
        (runner)(&serde_json::from_str(r#"
            [{"Setup":{"id":9154943204525373246,"public_id":"","username":"","email":"�","email_verified":true,"password_hash":"","created_at":"1905-02-08T18:09:19","is_superuser":true}}]
            "#).unwrap())
    }

    #[test]
    pub fn reg_6() {
        let runner = make_test_runner();
        (runner)(&serde_json::from_str(r#"
            [{"SubmitBallot":[{"pm":"","pm_score":4673012688971789599,"dpm":"","dpm_score":-233092927411513922,"lo":"","lo_score":-2251899989174176079,"dlo":"","dlo_score":-7457090019387053905,"mg":"","mg_score":-3515457699501683682,"gw":"","gw_score":-2143953046849901072,"mo":"","mo_score":-1935123147796626016,"ow":"","ow_score":-3877612264743874764},9083798814469052812]}]        "#).unwrap())
    }

    #[test]
    pub fn reg_7() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [{"Login":998}]
            "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_8() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [{"Setup":{"id":1051723031801100185,"public_id":"","username":"sZjD","email":"user@example.com","email_verified":true,"password_hash":"y<c5\\ 8W$E^3rxC\"B|:R","created_at":"1938-06-05T00:12:01","is_superuser":true}},{"Login":742},{"CreateGroup":{"id":369656251410584496,"public_id":"","name":"","website":null,"created_at":"1961-08-13T05:02:42"}}]
            "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_9() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [{"CreateSpar":{"id":3239868793834112434,"public_id":"","start_time":"2005-06-25T10:05:57","is_open":true,"release_draw":true,"spar_series_id":-821114819059254567,"created_at":"1985-11-23T01:53:32"}}]        "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_10() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": -6542184406933222305,
                      "public_id": "",
                      "username": "usefulAsciiString",
                      "email": "user@example.com",
                      "email_verified": false,
                      "password_hash": "Y@AO.QO",
                      "created_at": "2009-08-19T20:21:31",
                      "is_superuser": true
                    }
                  },
                  { "Login": 499 },
                  {
                    "CreateGroup": {
                      "id": 7268257846348737767,
                      "public_id": "",
                      "name": "user@example.com",
                      "website": null,
                      "created_at": "1996-12-13T08:27:12"
                    }
                  },
                  {
                    "CreateSparSeries": {
                      "id": -6157068167674692288,
                      "public_id": "",
                      "title": "",
                      "description": "",
                      "speakers_per_team": 8808831125861691341,
                      "group_id": -7144790252592802412,
                      "created_at": "2009-08-28T02:33:45"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_12() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "CreateGroup": {
                      "id": 4143838961841547521,
                      "public_id": "Ѵg",
                      "name": "+@Z.XD",
                      "website": null,
                      "created_at": "2010-03-24T06:31:55"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_11() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": -3429348441404813235,
                      "public_id": "",
                      "username": "OYUO",
                      "email": "user@example.com",
                      "email_verified": true,
                      "password_hash": "s9YFT/68w/>nzQV)EEcCc]E5jBE}On,Yr7FngW+`)3lGb@3xn[QL,I9@$(#j2mvYTn<e=\\R!&E&",
                      "created_at": "1914-11-03T04:01:54",
                      "is_superuser": true
                    }
                  },
                  { "Login": 499 },
                  {
                    "CreateGroup": {
                      "id": 7557077496156179134,
                      "public_id": "�",
                      "name": "H@P.PPCM",
                      "website": null,
                      "created_at": "1904-09-15T05:42:33"
                    }
                  },
                  {
                    "CreateSparSeries": {
                      "id": 1258516425073998925,
                      "public_id": ")����-K",
                      "title": "Person Lastname4",
                      "description": null,
                      "speakers_per_team": 3829528898168329212,
                      "group_id": -420942063879981829,
                      "created_at": "2003-07-22T12:52:56"
                    }
                  },
                  {
                    "AddMember": {
                      "id": 4236213730049836394,
                      "public_id": "%�\\\u001f",
                      "name": "person1116460962384346220558011",
                      "email": "Person Lastname6445912",
                      "spar_series_id": 6877399957622633576,
                      "created_at": "1991-02-22T01:40:43"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_13() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": -3429348441404813235,
                      "public_id": "",
                      "username": "OYUO",
                      "email": "user@example.com",
                      "email_verified": true,
                      "password_hash": "user@example.com",
                      "created_at": "1914-11-03T04:01:54",
                      "is_superuser": true
                    }
                  },
                  {
                    "CreateGroup": {
                      "id": 7557077496156179134,
                      "public_id": "�",
                      "name": "H@P.PPCM",
                      "website": null,
                      "created_at": "1904-09-15T05:42:33"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_14() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": 8609267956224745257,
                      "public_id": "",
                      "username": "WDPE",
                      "email": "user@exampe.com",
                      "email_verified": false,
                      "password_hash": "jS}0D:AkAoHo9\\BQq\"7a5o",
                      "created_at": "1995-07-04T13:44:34",
                      "is_superuser": false
                    }
                  },
                  {
                    "CreateGroup": {
                      "id": -2728873495393769222,
                      "public_id": "",
                      "name": "I6q\\Mr4($J5",
                      "website": null,
                      "created_at": "2035-08-12T09:20:42"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }

    #[test]
    pub fn reg_15() {
        let runner = make_test_runner();
        (runner)(
            &serde_json::from_str(
                r#"
                [
                  {
                    "Setup": {
                      "id": -2262268559300272614,
                      "public_id": "",
                      "username": "IOPA",
                      "email": "user@example.com",
                      "email_verified": false,
                      "password_hash": "Person Lastname5",
                      "created_at": "1938-02-21T22:31:11",
                      "is_superuser": true
                    }
                  },
                  {
                    "CreateGroup": {
                      "id": 6486003193559178599,
                      "public_id": "",
                      "name": "K@E.GMWP",
                      "website": "",
                      "created_at": "1973-08-03T06:48:53"
                    }
                  }
                ]
                "#,
            )
            .unwrap(),
        )
    }
}
