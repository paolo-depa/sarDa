# sarDa

One Rust executable: SA files in, Grafana URL out. It converts with `sadf` in a container, serves a Prometheus-compatible API on loopback, and starts a provisioned Grafana container. Users read `README.md`; vocabulary is in `GLOSSARY.md`; the reasons behind the shape are in `docs/adr/`. Read the ADRs before changing architecture, networking, dependencies or the data path.

## Verify

- `cargo test` is hermetic: fake `podman`/`docker` scripts, a recorded `sadf` stream, no network, no images.
- `SARDA_E2E=1 cargo test --test cli e2e_with_real_grafana` runs the real thing. It needs podman plus both images locally: `cargo run -q -- --print-containerfile | podman build -t localhost/sarda-sysstat:latest -f - .` and the Grafana image named in `src/cli.rs` (`DEFAULT_GRAFANA_IMAGE`).
- Run the E2E test after touching `grafana.rs`, `dashboard.rs`, `api.rs` or the Grafana environment variables. Unit tests cannot see Grafana's behaviour.

## Data path

`sadf.rs` (header probe per file, then a marker stream of `sadf -d` output) → `dataset.rs` (merge, window; one `Table` per Activity) → `store.rs` (labelled Series) → `api.rs` + `promql.rs` (Grafana reads here), `dashboard.rs` (generated JSON), `csvexport.rs` (`--export-csv`). `session.rs` orchestrates; `grafana.rs` owns the Grafana container and its cleanup.

## Rules that are not visible in the code

- **The probe decides what is skippable, not the file name.** `sadf -j -H` gives a file's host and `file-date`; `session::skip_bounds` keeps one day of slack around the window so a file is only skipped when its own timezone cannot bring it inside. Activities are skipped in the shell script (`tests/script.rs` covers it); never re-add a filename-based guess.
- **The catalog is the only place that knows Activities.** Add or change one in `src/catalog.rs`; parsing, metric names, panels and docs follow. Then refresh the README table: `cargo run -q -- --print-catalog` (a test fails while it is stale). Panel titles come from `description`, so keep descriptions unique.
- **Air-gapped.** No run-time network use beyond the single `pull` attempt in `session::ensure_image`. No plugin installs, update checks or telemetry; the Grafana `GF_*` variables in `grafana.rs` switch those off.
- **Loopback only.** Everything binds `127.0.0.1`. Anonymous Admin in Grafana is only acceptable because of that (ADR 0005); never combine it with another bind address.
- **Unsupported PromQL must error, never approximate.** `promql.rs` rejects what it cannot evaluate with an explanatory message; Grafana shows it in the panel.
- **Panels follow collection, not values.** An Activity recorded in the SA files gets its row and panels even when every sample is zero; an Activity the files lack gets none. Do not reintroduce value-based filtering: a flat zero chart is a finding. `tests/golden.rs` asserts this in both directions for every catalog Activity.
- **stdout is the result channel**: the URL, or JSON events with `--json`. Everything else goes to stderr through `Reporter`. Never `println!` elsewhere.
- **Exit codes are a contract**: `error::Kind` maps to the table in the README, and a test checks they agree. Add a `Kind` and a README row together.
- **Time is UTC end to end.** `sadf -d` emits UTC; `--start/--end` without an offset mean UTC.
- **Cleanup must survive SIGTERM and Ctrl-C** in every phase (`session::install_signal_handler`). A new container or temp path needs a matching removal there and in `Grafana::cleanup`.
- **No async runtime.** New crates must be vendorable: check with `scripts/dist.sh` (ADR 0004).

## Fixtures

`tests/fixtures/sadf.stream` is recorded `sadf` output, so tests need no `sadf`. `tests/fixtures/golden/` holds the CSV files the retired Python tool produced from the same input; `tests/golden.rs` holds the Rust export to them. Change that output format only on purpose, and update the golden files in the same commit. `SARDA_SADF_HOST=1` runs `sadf` on the host instead of the container, for recording fixtures only; `tests/fixtures/README.md` has the commands.
