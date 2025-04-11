#![feature(coverage_attribute)]

pub mod ballot;
pub mod email;
pub mod group;
pub mod magic_link;
pub mod room;
/// Database schema
pub mod schema;
pub mod spar;
pub mod user;

use rocket_sync_db_pools::database;

#[database("database")]
pub struct DbConn(rocket_sync_db_pools::diesel::SqliteConnection);
