# Ship sarDa as a single static Rust executable

Users must get from an SA file to a dashboard with one binary and no language runtime. We rewrote the former Python/pandas script in Rust, built as a static musl binary for linux amd64 and arm64, with the dashboard and provisioning files embedded. Go was the other serious candidate; Rust was chosen by the maintainer. Crates are few and vendored so the project builds offline (see ADR 0004).
