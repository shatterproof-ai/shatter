# rust-unavailable

Exercises str-jeen.13 — unavailable Rust frontend in a mixed-language run.

The broad-run gate runs `shatter scan` against this fixture with `PATH=""`
and `cwd` set to a tempdir, so neither `find_on_path("shatter-rust")` nor
the local `./shatter-rust/target/...` candidates can resolve. The CLI must
surface a structured `frontend_unavailable` / `skipped_by_unavailable_frontend`
status (and exit zero in mixed-language mode) rather than panicking.

The technique mirrors `shatter-cli/tests/rust_frontend_availability_test.rs`.
