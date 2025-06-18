use arbitrary::Arbitrary;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use fuzzcheck::{mutators::option::OptionMutator, DefaultMutator};
use fuzzcheck_util::{
    chrono_mutators::{naive_date_time_mutator, NaiveDateTimeMutator},
    useful_string_mutator::{useful_string_mutator, UsefulStringMutator},
};
use once_cell::sync::Lazy;
use regex::Regex;
use rocket::{
    http::{Cookie, CookieJar, Status},
    outcome::try_outcome,
    request::{self, FromRequest},
    Request,
};
use serde::{Deserialize, Serialize};

use crate::{
    schema::{self},
    DbConn,
};

pub const LOGIN_COOKIE: &str = "jeremy_bearimy";

#[derive(
    Debug, Queryable, Serialize, Deserialize, Clone, Arbitrary, DefaultMutator,
)]
pub struct User {
    pub id: i64,
    pub public_id: String,
    #[field_mutator(OptionMutator<String, UsefulStringMutator> = { OptionMutator::new(useful_string_mutator()) })]
    pub username: Option<String>,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub email: String,
    pub email_verified: bool,
    #[field_mutator(UsefulStringMutator = { useful_string_mutator() })]
    pub password_hash: String,
    #[field_mutator(NaiveDateTimeMutator = { naive_date_time_mutator() })]
    pub created_at: NaiveDateTime,
    pub is_superuser: bool,
    // todo: what resources should this restrict? Currently this just suspends
    // groups
    pub may_create_resources: bool,
}

type WithName<'a> =
    diesel::dsl::Eq<crate::schema::users::username, Option<&'a str>>;

type WithPublicId<'a> =
    diesel::dsl::Eq<crate::schema::users::public_id, &'a str>;

pub fn is_valid_email(string: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r#"(?:[a-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[a-z0-9!#$%&'*+/=?^_`{|}~-]+)*|"(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21\x23-\x5b\x5d-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])*")@(?:(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z0-9](?:[a-z0-9-]*[a-z0-9])?|\[(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?|[a-z0-9-]*[a-z0-9]:(?:[\x01-\x08\x0b\x0c\x0e-\x1f\x21-\x5a\x53-\x7f]|\\[\x01-\x09\x0b\x0c\x0e-\x7f])+)\])"#
    ).unwrap()
    });
    RE.is_match(string)
}

#[cfg(test)]
#[test]
fn test_email() {
    assert!(is_valid_email("hello@example.com"))
}

impl User {
    pub fn with_public_id(pid: &str) -> WithPublicId<'_> {
        crate::schema::users::public_id.eq(pid)
    }

    pub fn with_name(name: &str) -> WithName<'_> {
        crate::schema::users::username.eq(Some(name))
    }

    pub fn validate_email(email: &str) -> bool {
        is_valid_email(email)
    }

    pub fn validate_username(username: &str) -> bool {
        (username.chars().count() > 3)
            && username.chars().all(|c| c.is_ascii() && c.is_alphabetic())
    }

    pub fn validate_password(password: &str) -> bool {
        password.len() > 6
    }
}

#[derive(Debug)]
pub enum AuthError {
    CookieMissingOrMalformed,
    NoDatabase,
    Unauthorized,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoginSession {
    id: i64,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = AuthError;

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, AuthError> {
        let db = try_outcome!(request
            .guard::<DbConn>()
            .await
            .map_error(|(t, _)| (t, AuthError::NoDatabase)));

        let login_cookie = match request.cookies().get_private(LOGIN_COOKIE) {
            Some(cookie) => cookie,
            None => {
                return rocket::request::Outcome::Forward(Status::Unauthorized);
            }
        };

        let login: LoginSession =
            match serde_json::from_str(&login_cookie.value()) {
                Ok(t) => t,
                Err(_) => {
                    // TODO: log the error so that these can be easily resolved

                    // we need to remove cookie if incorrectly formatted, as they
                    // will otherwise persist and prevent the user from logging in
                    request.cookies().remove_private(LOGIN_COOKIE);
                    return rocket::request::Outcome::Forward(
                        Status::Unauthorized,
                    );
                }
            };

        let user: Option<User> = match db
            .run(move |conn| {
                schema::users::table
                    .filter(schema::users::id.eq(login.id))
                    .first(conn)
                    .optional()
            })
            .await
        {
            Ok(Some(user)) => Some(user),
            Ok(None) => None,
            Err(_) => {
                return rocket::request::Outcome::Error((
                    Status::InternalServerError,
                    AuthError::NoDatabase,
                ));
            }
        };

        match user {
            Some(user) => return rocket::request::Outcome::Success(user),
            None => {
                return rocket::request::Outcome::Error((
                    Status::Unauthorized,
                    AuthError::Unauthorized,
                ))
            }
        }
    }
}

pub fn set_login_cookie<'r>(id: i64, jar: &CookieJar) {
    jar.add_private({
        Cookie::new(
            LOGIN_COOKIE,
            serde_json::to_string(&LoginSession { id }).unwrap(),
        )
    });
}
