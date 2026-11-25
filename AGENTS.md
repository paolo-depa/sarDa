# sarDa

One Rust executable: SA files in, Grafana URL out. The code is the source of truth: when this file or anything under `docs/agents/` disagrees with the code, the code wins and the doc is the bug. A test checks that the paths and `module::name` pointers written there still exist.

Start at `src/lib.rs`, whose crate doc maps the data path. Vocabulary is in `GLOSSARY.md`: use its terms, and add one when you name a new concept. Users read `README.md`.

Load when:

- `docs/adr/`: before changing architecture, networking, dependencies, the sysstat image or the data path. They say why the shape is what it is.
- `docs/agents/verify.md`: before running, writing or changing tests, or touching `src/grafana.rs`, `src/dashboard.rs` or `src/api.rs`.
- `docs/agents/constraints.md`: before adding a dependency, network access, a port, a container, an output, an error or a metric.
