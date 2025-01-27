pub mod group;
pub mod magic_link;
/// Database schema
pub mod schema;
pub mod spar;
pub mod user;

use rocket_sync_db_pools::database;

#[database("database")]
pub struct DbConn(rocket_sync_db_pools::diesel::SqliteConnection);
