use db::{group::GroupMember, schema::group_members, user::User};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::resources::GroupRef;

#[derive(Debug)]
/// A permission for a given resource on the system.
pub enum Permission {
    /// Create any new resource in a given group, for example a spar series.
    CreateNewGroup,
    CreateNewResourceInGroup(GroupRef),
    DeleteResourceInGroup(GroupRef),
    ModifyResourceInGroup(GroupRef),
    RegisterAsNewUser,
    /// Edit the site-wide configuration.
    ModifyGlobalConfig,
}

/// Returns whether a requester has the requisite permission on the given
/// object.
///
/// TODO: could add reasons for permissions being denied
#[tracing::instrument(skip(conn))]
pub fn has_permission(
    user: Option<&User>,
    permission: &Permission,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> bool {
    match permission {
        Permission::CreateNewGroup => {
            user.map(|user| user.may_create_resources).unwrap_or(false)
        }
        Permission::CreateNewResourceInGroup(GroupRef(group_id)) => {
            check_modify_resource_in_group(user, conn, group_id)
        }
        Permission::ModifyResourceInGroup(GroupRef(group_id)) => {
            check_modify_resource_in_group(user, conn, group_id)
        }
        Permission::DeleteResourceInGroup(GroupRef(group_id)) => {
            check_delete_resource_in_group(user, conn, group_id)
        }
        Permission::RegisterAsNewUser => check_if_registrations_are_open(conn),
        Permission::ModifyGlobalConfig => match user {
            Some(user) => user.is_superuser,
            None => false,
        },
    }
}

#[tracing::instrument(skip(conn))]
fn check_if_registrations_are_open(
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
) -> bool {
    let disable_signups = db::schema::config::table
        .filter(db::schema::config::key.eq(&"disable_signups"))
        .select(db::schema::config::value)
        .first::<String>(conn)
        .optional()
        .unwrap()
        .map(|value| {
            assert!(value.parse::<u32>().is_ok());
            value == "1"
        })
        .unwrap_or(false);

    !disable_signups
}

#[tracing::instrument(skip(conn))]
fn check_delete_resource_in_group(
    user: Option<&User>,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    group_id: &i64,
) -> bool {
    let user = match user {
        Some(user) => user,
        None => return false,
    };

    let group_membership = match group_members::table
        .filter(group_members::user_id.eq(user.id))
        .filter(group_members::group_id.eq(group_id))
        .first::<GroupMember>(conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return false,
    };

    group_membership.has_signing_power
}

#[tracing::instrument(skip(conn))]
fn check_modify_resource_in_group(
    user: Option<&User>,
    conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    group_id: &i64,
) -> bool {
    let user = match user {
        Some(user) => user,
        None => return false,
    };

    let group_membership = match group_members::table
        .filter(group_members::user_id.eq(user.id))
        .filter(group_members::group_id.eq(group_id))
        .first::<GroupMember>(conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return false,
    };

    assert!(
        group_membership.has_signing_power as u8
            <= group_membership.is_admin as u8
    );

    group_membership.is_admin || group_membership.has_signing_power
}
