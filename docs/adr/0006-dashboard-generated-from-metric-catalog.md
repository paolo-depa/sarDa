# Generate the dashboard from a Metric catalog

The previous dashboard was 10k lines of hand-edited JSON with the device name `dev254-0` hardcoded in every per-device panel, so it only worked for one host. The dashboard JSON, the Sarda API metric names, the parsing rules and the panel reference documentation are now all derived from one Rust Metric catalog; per-CPU, per-device and per-interface panels use Grafana label variables. Adding an Activity is a one-place change.
