use std::sync::Arc;

use argon2::password_hash::SaltString;
use argon2::Argon2;
use argon2::PasswordHasher;
use db::invite::AccountInvite;
use db::schema::account_invites;
use db::{schema::users, user::User, DbConn};
use diesel::dsl::{insert_into, now};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use diesel::{Connection, RunQueryDsl};
use email::send_mail;
use maud::{html, Markup};
use rand::rngs::OsRng;
use rocket::form::Form;
use rocket::response::Redirect;
use tracing::Instrument;
use uuid::Uuid;

use crate::model::sync::id::gen_uuid;
use crate::request_ids::RequestId;
use crate::request_ids::TracingSpan;
use crate::{
    html::{error_403, page_of_body, page_title},
    permissions::has_permission,
};

#[get("/admin/invite")]
pub async fn send_invite_page(
    user: User,
    db: DbConn,
) -> Result<Markup, Markup> {
    db.run(|conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &crate::permissions::Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Err(error_403(
                    Some("Error: you are not authorized to view this page"),
                    Some(user),
                )));
            }

            let page = maud::html! {
                (page_title("Invite user to create an account"))
                (render_invite_form(None, None::<String>))
            };

            return Ok(Ok(page_of_body(page, Some(user))));
        })
        .unwrap()
    })
    .await
}

fn render_invite_form<T: ToString>(
    prev: Option<&InviteUserForm>,
    error: Option<T>,
) -> Markup {
    html! {
        @if let Some(error) = error {
            div class="alert alert-danger" {
                (error.to_string())
            }
        }
        form action="" method="post" class="container mt-4" {
            div class="form-group mb-3" {
                label for="email" class="form-label" { "Email: " }
                (match prev {
                    Some(prev) => {
                        html! {
                            input type="email" id="email" name="email" class="form-control" value=(prev.email) required;
                        }
                    }
                    None => {
                        html! {
                            input type="email" id="email" name="email" class="form-control"  required;
                        }
                    }
                })
            }
            input type="submit" value="Send Invite" class="btn btn-primary";
        }
    }
}

#[derive(FromForm)]
pub struct InviteUserForm {
    email: String,
}

#[post("/admin/invite", data = "<invite>")]
#[tracing::instrument(skip(user, db, invite, span))]
pub async fn do_invite_user(
    user: User,
    db: DbConn,
    invite: Form<InviteUserForm>,
    span: TracingSpan,
) -> Result<Markup, Markup> {
    let db = Arc::new(db);
    db.clone().run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if !has_permission(
                Some(&user),
                &crate::permissions::Permission::ModifyGlobalConfig,
                conn,
            ) {
                return Ok(Err(error_403(
                    Some("Error: you are not authorized to view this page"),
                    Some(user),
                )));
            }

            tracing::trace!("User has permission to send invite");

            if !User::validate_email(&invite.email)
            {
                return Ok(Err(page_of_body(
                    html! {
                        (page_title("Invite user to create an account"))
                        (render_invite_form(Some(&invite), Some("Error: the provided email is not a valid email address.")))
                    },
                    Some(user),
                )));
            }

            tracing::trace!("Email to send invite to passed validation.");

            if diesel::select(diesel::dsl::exists(
                users::table.filter(users::email.eq(&invite.email)),
            ))
            .get_result::<bool>(conn)
            .unwrap()
            {
                return Ok(Err(page_of_body(
                    html! {
                        (page_title("Invite user to create an account"))
                        (render_invite_form(Some(&invite), Some("Error: a user with that email is already registered!")))
                    },
                    Some(user),
                )));
            }

            tracing::trace!("No such user already exists.");

            let n = diesel::insert_into(account_invites::table)
                .values((
                    account_invites::public_id.eq(gen_uuid().to_string()),
                    account_invites::code.eq(Uuid::new_v4().to_string()),
                    // we convert the email address into a canonical form to try
                    // to prevent people from accidentally sending duplicate
                    // account creation emails
                    account_invites::email.eq(invite.email.trim().to_ascii_lowercase()),
                    account_invites::sent_by.eq(&user.id),
                    account_invites::created_at.eq(diesel::dsl::now),
                ))
                // todo: warn when sending duplicate invites
                .on_conflict_do_nothing()
                .execute(conn).unwrap();
            assert!(n <= 1);

            tracing::trace!("Did insert of new user.");

            let code = account_invites::table
                .filter(account_invites::email.eq(invite.email.trim().to_ascii_lowercase()))
                .select(account_invites::code)
                .first::<String>(conn)
                .unwrap();

            let link = format!("https://eldemite.net/invites/{}", code);
            send_mail(
                vec![(&invite.email, &invite.email)],
                "Account invite for eldemite.net",
                &html! {
                    p {
                        "This email contains an invitation to create an account on eldemite.net."
                    }
                    p {
                        "Please use "
                        a href=(link) {
                            "this link "
                        }
                        "to sign up."
                    }
                }.into_string(),
                &format!("Please use this link to create an account on eldemite.net: {link}"),
                db
            );

            return Ok(Ok(page_of_body(html! {
                (page_title("Invitation sent"))
                p {
                    "An invitation has been emailed to the person in question."
                }
            }, Some(user))))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

/// Checks that an invite with the given code exists and that no user with that
/// email already exists.
fn check_invite_is_valid(
    code: &str,
    conn: &mut (impl Connection<Backend = Sqlite>
              + diesel::connection::LoadConnection),
) -> bool {
    let invite = match account_invites::table
        .filter(account_invites::code.eq(&code))
        .first::<AccountInvite>(conn)
        .optional()
        .unwrap()
    {
        Some(invite) => invite,
        None => return false,
    };

    // can't invite users who already exist
    if diesel::select(diesel::dsl::exists(
        users::table
            .filter(users::email.eq(&invite.email.trim().to_ascii_lowercase())),
    ))
    .get_result::<bool>(conn)
    .unwrap()
    {
        return false;
    };

    true
}

#[get("/invites/<invite_code>")]
pub async fn accept_invite_page(
    invite_code: &str,
    user: Option<User>,
    db: DbConn,
) -> Result<Markup, Markup> {
    let invite_code = invite_code.to_string();
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if user.is_some() {
                return Ok(Err(error_403(Some("Error: you cannot accept an invite as a logged-in user."), user)))
            }

            if !check_invite_is_valid(&invite_code, conn) {
                return Ok(Err(error_403(Some("Error: that invite is not valid."), user)))
            }

            let invite = account_invites::table.filter(account_invites::code.eq(&invite_code)).first::<AccountInvite>(conn).unwrap();

            let page = html! {
                (page_title("Create an account"))
                (create_account_form(&invite.email, None, None))
            };

            return Ok(Ok(
                page_of_body(page, user)
            ))
        }).unwrap()
    })
    .await
}

fn create_account_form(
    email: &str,
    prev: Option<&CreateAccountFromInvitationForm>,
    error: Option<&str>,
) -> Markup {
    html! {
        @if let Some(e) = error {
            div class="alert alert-danger" {
                (e)
            }
        }
        form method="post" class="container mt-4" {
            div class="form-group mb-3" {
                label for="username" class="form-label" { "Username" }
                input type="text" id="username" name="username" class="form-control" value=(prev.map(|f| f.username.clone()).unwrap_or_default());
            }
            div class="form-group mb-3" {
                label for="email" class="form-label" { "Email" }
                input type="email" id="email" name="email" class="form-control" value=(email);
            }
            div class="form-group mb-3" {
                label for="password" class="form-label" { "Password" }
                input type="password" id="password" name="password" class="form-control" value=(prev.map(|f| f.password.clone()).unwrap_or_default());
            }
            div class="form-group mb-3" {
                label for="password2" class="form-label" { "Confirm Password" }
                input type="password" id="password2" name="password2" class="form-control" value=(prev.map(|f| f.password2.clone()).unwrap_or_default());
            }
            button type="submit" class="btn btn-primary" { "Register" }
        }
    }
}

#[derive(FromForm)]
pub struct CreateAccountFromInvitationForm {
    username: String,
    email: String,
    password: String,
    password2: String,
}

#[post("/invites/<invite_code>", data = "<form>")]
pub async fn do_accept_invite(
    form: Form<CreateAccountFromInvitationForm>,
    invite_code: &str,
    db: DbConn,
    user: Option<User>,
    req_id: RequestId,
) -> Result<Redirect, Markup> {
    let invite_code = invite_code.to_string();
    db.run(move |conn| {
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            if user.is_some() {
                return Ok(Err(create_account_form(
                    &form.email,
                    Some(&form),
                    Some("Error: you can not accept an invite as a logged-in user."),
                )));
            }

            if !check_invite_is_valid(&invite_code, conn) {
                return Ok(Err(error_403(Some("Error: that invite is not valid (it may have expired)."), user)));
            }
            let invite = account_invites::table.filter(account_invites::code.eq(invite_code)).first::<AccountInvite>(conn).unwrap();

            if invite.email != form.email.to_ascii_lowercase().trim() {
                return Ok(
                    Err(
                        error_403(Some(format!("Error: the email address from your form
                                        submission doesn't match the invite
                                        code. (If you believe this is an error,
                                        please contact the system administrator
                                        and quote `request_id={}`).", req_id.to_string())), user))
                    );
            }

            if form.password != form.password2 {
                return Ok(Err(create_account_form(
                    &form.email,
                    Some(&form),
                    Some("Error: your passwords do not match."),
                )));
            }

            if !User::validate_username(&form.username) {
                return Ok(Err(create_account_form(
                    &form.email,
                    Some(&form),
                    Some(
                        "Error: names should consist exclusively of letters
                         and spaces.",
                    ),
                )));
            }

            let salt = SaltString::generate(&mut OsRng);

            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(form.password.as_bytes(), &salt)
                .unwrap()
                .to_string();

            let n = insert_into(users::table)
                .values((
                    users::public_id.eq(gen_uuid().to_string()),
                    users::username.eq(&form.username),
                    users::email.eq(&invite.email.trim().to_ascii_lowercase()),
                    users::email_verified.eq(false),
                    users::created_at.eq(now),
                    users::password_hash.eq(&password_hash),
                    users::is_superuser.eq(false),
                    users::may_create_resources.eq(true),
                ))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);

            return Ok(Ok(Redirect::to("/profile")))
        })
        .unwrap()
    })
    .await
}
