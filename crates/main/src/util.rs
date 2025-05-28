use db::{DbConn, DbWrapper};
use diesel::Connection;
use once_cell::sync::Lazy;
use rand::{distributions::Alphanumeric, Rng};
use regex::Regex;
use tracing::Instrument;

use crate::request_ids::TracingSpan;

/// Runs the given function inside a database transaction.
///
/// Important notes:
/// - SQLite locks the database, so long-running requests should not be run
///   inside a transaction as this will starve other transactions.
pub async fn tx<T>(
    span: TracingSpan,
    db: DbConn,
    f: impl FnOnce(&mut DbWrapper) -> T + Send + 'static,
) -> T
where
    T: Send + Sync + 'static,
{
    let span1 = span.0.clone();
    db.run(move |conn| {
        let _guard = span1.enter();
        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            Ok(f(conn))
        })
        .unwrap()
    })
    .instrument(span.0)
    .await
}

pub fn short_random(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

pub fn is_valid_email(string: &str) -> bool {
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r"(?m)^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$",
        )
        .unwrap()
    });
    RE.is_match(string)
}

#[cfg(test)]
mod test_email_regex {
    use crate::util::is_valid_email;

    #[test]
    fn test_simple_test_email() {
        assert!(is_valid_email("judge1@example.com"))
    }
}
