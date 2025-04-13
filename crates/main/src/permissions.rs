use db::{group::GroupMember, schema::group_members, user::User};
use diesel::{prelude::*, SqliteConnection};

use crate::resources::GroupRef;

#[derive(Debug)]
/// A permission for a given resource on the system.
pub enum Permission {
    /// Create any new resource in a given group, for example a spar series.
    CreateNewResourceInGroup(GroupRef),
    DeleteResourceInGroup(GroupRef),
    ModifyResourceInGroup(GroupRef),
}

/// Returns whether a requester has the requisite permission on the given
/// object.
///
/// TODO: could add reasons for permissions being denied
pub fn has_permission(
    user: Option<&User>,
    permission: &Permission,
    conn: &mut SqliteConnection,
) -> bool {
    match permission {
        Permission::CreateNewResourceInGroup(GroupRef(group_id))
        | Permission::ModifyResourceInGroup(GroupRef(group_id)) => {
            check_non_delete_action_in_group(user, conn, group_id)
        }
        Permission::DeleteResourceInGroup(GroupRef(group_id)) => {
            check_delete_resource_in_group(user, conn, group_id)
        }
    }
}

fn check_delete_resource_in_group(
    user: Option<&User>,
    conn: &mut SqliteConnection,
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

fn check_non_delete_action_in_group(
    user: Option<&User>,
    conn: &mut SqliteConnection,
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

    group_membership.is_admin || group_membership.has_signing_power
}
