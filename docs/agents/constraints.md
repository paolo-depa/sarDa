# Constraints

Load when: you add a dependency, network access, a port, a container, an output, an error or a metric.

Each rule names the one place that states it. Read that place, not this line; if they disagree, that place wins and this line is the bug.

| Rule | Source of truth |
|---|---|
| Air-gapped: no run-time network use except pulling a missing image (`sarda prepare` pulls and builds on purpose); no plugins, update checks or telemetry | `runtime::pull`; the `GF_*` list in `src/grafana.rs`; `README.md` ("What you need") |
| Everything binds loopback; Grafana's anonymous Admin depends on it | ADR 0005; `tests/docs.rs` forbids `0.0.0.0` and `[::]` in `src/` |
| stdout carries only the URL or JSON events | `session::Reporter`; `tests/docs.rs` limits `print!`/`println!` to `src/session.rs` and `src/main.rs` |
| Exit codes are a contract | `error::Kind`; `tests/docs.rs` compares `README.md` and `--help` with it |
| The catalog is the only place that knows Activities | `catalog` module doc; `tests/docs.rs` keeps the README table current |
| Only the table module interprets `sadf` cells | `table` module doc |
| Containers and temp paths are created only through `Resources` | `resources` module doc |
| Panels follow what was collected, not values | `dashboard` module doc; `tests/golden.rs` |
| Unsupported PromQL is an error, never an approximation | `promql` module doc |
| A file's day comes from its header, not its name; incompatibility is never inferred from empty output | `session::skip_bounds`; ADR 0007 |
| The parser image is the delivered sysstat, and its script stays POSIX `sh` plus coreutils | ADR 0007; the script test in `src/sadf/script.rs` |
| No async runtime; crates must be vendorable | ADR 0004; `scripts/dist.sh` |
