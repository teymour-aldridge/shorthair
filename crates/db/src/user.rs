use chrono::NaiveDateTime;
use diesel::prelude::*;
use rocket::{
    http::{Cookie, CookieJar, Status},
    outcome::try_outcome,
    request::{self, FromRequest},
    Request,
};
use serde::Serialize;

use crate::{
    schema::{self, spar_room_adjudicator, spar_rooms, spars},
    DbConn,
};

pub const LOGIN_COOKIE: &str = "jeremy_bearimy";

#[derive(Debug, Queryable, Serialize, Clone)]
pub struct User {
    pub id: i64,
    pub public_id: String,
    pub username: Option<String>,
    pub email: String,
    pub email_verified: bool,
    pub password_hash: Option<String>,
    pub created_at: NaiveDateTime,
    pub is_superuser: bool,
}

type WithName<'a> =
    diesel::dsl::Eq<crate::schema::users::username, Option<&'a str>>;

type WithPublicId<'a> =
    diesel::dsl::Eq<crate::schema::users::public_id, &'a str>;

impl User {
    pub fn with_public_id(pid: &str) -> WithPublicId {
        crate::schema::users::public_id.eq(pid)
    }

    pub fn with_name(name: &str) -> WithName {
        crate::schema::users::username.eq(Some(name))
    }
}

#[diesel::dsl::auto_type]
/// Find the record (if any) which stores information about this user as an
/// adjudicator in this session.
pub fn adj_of_user_in_session(user: &User, session_id: String) -> _ {
    let user_id: i64 = user.id;
    spar_room_adjudicator::table
        .filter(spar_room_adjudicator::user_id.eq(user_id))
        .inner_join(spar_rooms::table.inner_join(spars::table))
        .filter(spars::public_id.eq(session_id))
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
                return rocket::request::Outcome::Error((
                    Status::BadRequest,
                    AuthError::CookieMissingOrMalformed,
                ));
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
                    return rocket::request::Outcome::Error((
                        Status::BadRequest,
                        AuthError::CookieMissingOrMalformed,
                    ));
                }
            };

        let user = match db
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
