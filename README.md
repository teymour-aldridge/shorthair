# Shorthair

A series of debating-related pieces of software. These are currently all a
work-in-progress. Please report any bugs (or other feedback/suggestions) to
Teymour Aldridge. You can email me (teymour@reasoning.page), or in more typical
debater fashion text me on Facebook Messenger.

## Packages

- spar generator - a spar generator. Produces a QR code you can send to
  people to sign up. Then uses integer linear programming in order to
  generate a draw which creates pro-am pairings.
- Model-based test for the application, along the lines of
  [this excellent article](https://concerningquality.com/model-based-testing/)


## Planned things

- [ ] High-quality adjudicator allocation, a lá
      [Adjumo](https://czlee.nz/debating/adjumo.pdf), intended to automatically
      allocate adjudicators for large tournaments according to any set of
      preferences.
      - The key concern here is speed - this should run in (at most) 10 minutes
        for it to be useful. Prior attempts struggled with this. More extensive
        testing before tournaments would go a long way here, I imagine.
- [ ] A reverse proxy/cacheing layer for Tabbycat. Currently Tabbycat is quite
      slow and can struggle with excessive load. This will save previous
      responses from Tabbycat and use them where appropriate. Specific things:
      - Make sure to cache all the private URL pages, as this can place a
        significant amount of strain on servers.
      - Cache rendered (i.e. the first time the page is loaded, run it in a
        web browser on the server) large pages on the separate server and then
        serve copies of them to requesting clients.
      - By acting as a reverse proxy, we can identify API calls which require us
        to evict parts of the cache, and do so automatically.

## Development notes

### Overview

The application is intentionally simple. All data is stored in an SQLite
database. Database migrations are stored in `migrations`. To run the migrations
run `diesel migration run --database-url sqlite://sqlite.db`. To run the server
use `cargo run`.

### Running tests

To run the tests, use

```
cargo test
```

Note that this only runs the manual tests, and some saved output from previous
runs of the fuzzer (read: automatic test case generator). To run the fuzzer run
the command

```
TODO
```
