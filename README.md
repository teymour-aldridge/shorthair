# Shorthair

Software useful for debating.

- Spar generation: generates allocations for debating society weekly sessions.
  This makes it possible to easily generate pro-am allocations (between more
  experienced and less experienced debaters).

Please report any bugs (or other feedback/suggestions) on this page. You can
also email me (teymour@reasoning.page).

## Development notes

### Overview

The application is intentionally simple. All data is stored in an SQLite
database. Database migrations are stored in `migrations`. To run the migrations
run `diesel migration run --database-url sqlite://sqlite.db`. To run the server
use `cargo run`.

### Running tests

To run the tests, use

```bash
cargo test
```

Note that this only runs a series of hand-written test sequences, as well as
saved regressions from testing against the model of the application.

To run the fuzzer, which compares the actual implementation to a simple oracle,
use the command

```bash
cd crates/main && cargo fuzzcheck model::do_model_test
```
