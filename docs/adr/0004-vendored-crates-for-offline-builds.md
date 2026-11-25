# Vendor a small set of crates

The project must build on air-gapped hosts. We use a few well-known crates (CLI parsing, JSON, a blocking HTTP server, a regex engine, signal handling) with no async runtime, and `scripts/dist.sh` bundles the `cargo vendor` output (about 46 MB) with a matching `.cargo/config.toml` into a release tarball, so `cargo build --release --offline` works after extracting it. The vendor directory is not committed, to keep the git history small. Std-only was rejected because hand-written HTTP/JSON/regex would add more bugs than it removes.
