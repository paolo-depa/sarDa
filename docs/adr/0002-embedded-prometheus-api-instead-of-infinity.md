# Serve data through an embedded Prometheus-compatible API

The first design used Grafana's Infinity plugin fetching CSV files over HTTP. Infinity is not bundled with Grafana and installing it needs internet, which is unavailable in the target (air-gapped) environment. Grafana's built-in Prometheus datasource needs nothing extra, so the executable itself implements the small part of the Prometheus HTTP API Grafana needs (`query_range`, `query`, `labels`, `label/<name>/values`, `series`, build info) over in-memory Series.

## Considered Options

- Bundle the Infinity plugin zip in the binary: keeps the old dashboard but adds ~30 MB and a frozen plugin version.
- VictoriaMetrics or Postgres sidecar: full query power, but a second image to pre-load.

## Consequences

Only a PromQL subset is evaluated: selectors with `=`, `!=`, `=~`, `!~`, and `sum/avg/min/max/count` with `by`/`without`. Anything else must return a Prometheus-style error. Labels replace the hardcoded device names of the old dashboard.
