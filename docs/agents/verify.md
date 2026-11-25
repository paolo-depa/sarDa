# Verifying changes

Load when: you run, write or change tests, or touch `src/grafana.rs`, `src/dashboard.rs` or `src/api.rs`.

- `cargo test` is hermetic: fake `podman`/`docker` scripts, a recorded `sadf` stream, no network, no images. Run it after every change.
- `SARDA_E2E=1 cargo test --test cli e2e_with_real_grafana` runs against real podman and the real images; the test's doc comment says what it needs, and `README.md` ("Prepare the images") says how to build them. Run it after touching `src/grafana.rs`, `src/dashboard.rs`, `src/api.rs` or the Grafana environment variables: unit tests cannot see Grafana's behaviour.
- Fixtures and how to regenerate them: `tests/fixtures/README.md`.
- Tests that need files use `testutil::Scratch` (`src/testutil.rs`), which removes its directory when dropped; do not create directories under the system temp dir by hand.
