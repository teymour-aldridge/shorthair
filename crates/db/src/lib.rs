#![feature(coverage_attribute)]

pub mod ballot;
pub mod config;
pub mod draft_draw;
pub mod email;
pub mod group;
pub mod invite;
pub mod magic_link;
pub mod room;
/// Database schema
pub mod schema;
pub mod spar;
pub mod user;

use diesel::connection::{
    DefaultLoadingMode, LoadConnection, TransactionManager,
};
use diesel::expression::QueryMetadata;
use diesel::migration::{MigrationConnection, CREATE_MIGRATIONS_TABLE};
use diesel::query_builder::Query;
use diesel::{
    connection::{
        AnsiTransactionManager, ConnectionSealed, Instrumentation,
        SimpleConnection,
    },
    query_builder::{QueryFragment, QueryId},
    r2d2::{ConnectionManager, ManageConnection},
    sqlite::Sqlite,
    Connection, ConnectionResult, QueryResult, SqliteConnection,
};
use diesel::{sql_query, RunQueryDsl};
use rocket::{Build, Rocket};
use rocket_sync_db_pools::{database, Config, PoolResult, Poolable};

#[database("database")]
pub struct DbConn(DbWrapper);

pub struct DbWrapper(SqliteConnection);

impl SimpleConnection for DbWrapper {
    #[tracing::instrument(skip(self, query))]
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        self.0.batch_execute(query)?;

        Ok(())
    }
}

impl ConnectionSealed for DbWrapper {}

impl Connection for DbWrapper {
    type Backend = Sqlite;
    type TransactionManager = AnsiTransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<DbWrapper> {
        Ok(DbWrapper(SqliteConnection::establish(database_url)?))
    }

    #[tracing::instrument(skip(self, f))]
    fn transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<diesel::result::Error>,
    {
        Self::TransactionManager::transaction(self, f)
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Sqlite> + QueryId,
    {
        self.0.execute_returning_count(source)
    }

    fn transaction_state(&mut self) -> &mut Self::TransactionManager {
        self.0.transaction_state()
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        self.0.instrumentation()
    }

    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        self.0.set_instrumentation(instrumentation)
    }
}

impl LoadConnection<DefaultLoadingMode> for DbWrapper {
    type Cursor<'conn, 'query>
        = <SqliteConnection as LoadConnection<DefaultLoadingMode>>::Cursor<
        'conn,
        'query,
    >
    where
        Self: 'conn;
    type Row<'conn, 'query>
        = <SqliteConnection as LoadConnection<DefaultLoadingMode>>::Row<
        'conn,
        'query,
    >
    where
        Self: 'conn;

    #[tracing::instrument(skip(self, source))]
    fn load<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<Self::Cursor<'conn, 'query>>
    where
        T: Query + QueryFragment<Self::Backend> + QueryId + 'query,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        self.0.load(source)
    }
}

impl MigrationConnection for DbWrapper {
    fn setup(&mut self) -> QueryResult<usize> {
        sql_query(CREATE_MIGRATIONS_TABLE).execute(self)
    }
}

pub struct DbWrapperManager {
    manager: ConnectionManager<SqliteConnection>,
}

impl ManageConnection for DbWrapperManager {
    type Connection = DbWrapper;

    type Error = diesel::r2d2::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.manager.connect().map(DbWrapper)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        self.manager.is_valid(&mut conn.0)
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        self.manager.has_broken(&mut conn.0)
    }
}

impl Poolable for DbWrapper {
    type Manager = DbWrapperManager;

    type Error = std::convert::Infallible;

    fn pool(db_name: &str, rocket: &Rocket<Build>) -> PoolResult<Self> {
        use diesel::connection::SimpleConnection;
        use diesel::r2d2::{
            ConnectionManager, CustomizeConnection, Error, Pool,
        };

        #[derive(Debug)]
        struct Customizer;

        struct ConnectionTracer;

        impl diesel::connection::Instrumentation for ConnectionTracer {
            fn on_connection_event(
                &mut self,
                event: diesel::connection::InstrumentationEvent<'_>,
            ) {
                match event {
                    diesel::connection::InstrumentationEvent::StartQuery {
                        query,
                        ..
                    } => {
                        tracing::trace!("Started running query {query:?}");
                    }
                    diesel::connection::InstrumentationEvent::FinishQuery {
                        query,
                        error,
                        ..
                    } => {
                        if let Some(error) = error {
                            tracing::warn!("Encountered an error when running query {query} (error: {error})");
                        }
                    }
                    _ => (),
                }
            }
        }

        impl CustomizeConnection<DbWrapper, Error> for Customizer {
            fn on_acquire(&self, conn: &mut DbWrapper) -> Result<(), Error> {
                conn.0.set_instrumentation(ConnectionTracer);

                conn.0
                    .batch_execute(
                        "\
                    PRAGMA journal_mode = WAL;\
                    PRAGMA busy_timeout = 1000;\
                    PRAGMA foreign_keys = ON;\
                ",
                    )
                    .map_err(Error::QueryError)?;

                Ok(())
            }
        }

        let config = Config::from(db_name, rocket)?;
        let manager = DbWrapperManager {
            manager: ConnectionManager::new(&config.url),
        };
        let pool = Pool::builder()
            .connection_customizer(Box::new(Customizer))
            .max_size(config.pool_size)
            .connection_timeout(std::time::Duration::from_secs(
                config.timeout as u64,
            ))
            .build(manager)?;

        Ok(pool)
    }
}
