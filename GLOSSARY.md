# sarDa

sarDa turns binary `sa` files produced by sysstat into an interactive Grafana dashboard, in one command, on machines with no internet access.

## Language

**SA file**:
A binary system-activity file written by `sa1`/`sadc` (typically `saYYYYMMDD`), covering one day of one host.
_Avoid_: sar data, sar file, sa1 file, binary

**Activity**:
One kind of statistic that `sadf` can extract from an SA file, selected by a sar option such as `-n DEV` or `-r ALL` (for example network devices, memory, disks).
_Avoid_: aggregator, metric label, sar_param

**Dimension**:
The column that splits an Activity into several independent entities (`CPU`, `DEV`, `IFACE`, `INTR`, `TTY`), becoming a label on every Series.
_Avoid_: pivot column, key

**Metric**:
One numeric column of an Activity, exposed under a stable name such as `sarda_network_dev_rxkB_s`.
_Avoid_: value column, counter

**Series**:
All Samples of one Metric for one combination of label values (for example `rxkB_s` of interface `eth0`).
_Avoid_: timeseries, curve, line

**Sample**:
A single timestamped value of a Series.
_Avoid_: point, row, datapoint

**Metric catalog**:
The single source of truth listing every Activity, its Dimension, its Metrics, units and dashboard grouping. It drives parsing, the Sarda API, the generated dashboard and the panel reference.
_Avoid_: aggregators, config table

**Observation window**:
The time range kept after parsing, bounded by optional `--start`/`--end`, in UTC.
_Avoid_: time filter, date range

## Serving

**Session**:
One run of the executable: parse, serve, start the Viewer, wait for Ctrl-C, clean up.
_Avoid_: job, run, instance

**Viewer**:
The Grafana container started for a Session, preconfigured and removed when the Session ends.
_Avoid_: Grafana instance, dashboard container

**Sarda API**:
The Prometheus-compatible HTTP API served by the executable on loopback, from which the Viewer reads the Series.
_Avoid_: backend, datasource server, exporter

**Parser container**:
The throwaway container, built from the sysstat image, that runs `sadf` against the SA files.
_Avoid_: sadf container, converter

**Runtime**:
The container engine found on the host, either `podman` or `docker`.
_Avoid_: engine, docker (when podman is meant)

**CSV export**:
The optional per-Metric CSV files written with `--export-csv`, kept for workflows that predate the Viewer.
_Avoid_: output dir, dump
