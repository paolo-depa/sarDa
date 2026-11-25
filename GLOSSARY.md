# sarDa

sarDa turns binary `sa` files produced by sysstat into an interactive Grafana dashboard, in one command, on machines with no internet access.

## Language

**SA file**:
A binary file covering one day of one host's activity, written by `sadc` (usually through `sa1`); sysstat's man pages call it the *standard system activity daily data file* and name it `saDD` or `saYYYYMMDD`.
_Avoid_: sar data, sar file, sa1 file, binary, data file

**Activity**:
One group of statistics that sar can collect and report, selected with one of sar's activity flags such as `-n DEV` or `-r ALL` (for example network devices, memory, disks).
_Avoid_: aggregator, metric label, sar_param

**Dimension**:
The column that splits an Activity into several independent entities (`CPU`, `DEV`, `IFACE`, `INTR`, `TTY`), becoming a label on every Series.
_Avoid_: pivot column, key

**Metric**:
One numeric column of an Activity, exposed under a stable name such as `sarda_network_dev_rxkB_s`.
_Avoid_: value column, statistic, field, counter

**Series**:
All Samples of one Metric for one combination of label values (for example `rxkB_s` of interface `eth0`).
_Avoid_: timeseries, curve, line

**Sample**:
A single timestamped value of a Series.
_Avoid_: point, row, datapoint, record

**Metric catalog**:
The single source of truth listing every Activity, its Dimension, its Metrics, units and dashboard grouping. It drives parsing, the Sarda API, the generated dashboard and the panel reference.
_Avoid_: aggregators, config table

**Observation window**:
The time range kept after parsing, bounded by optional `--start`/`--end`, in UTC.
_Avoid_: time filter, date range

## Serving

**Session**:
One run of the executable: parse, serve, start the Grafana instance, wait for Ctrl-C, clean up.
_Avoid_: job, run, instance

**Grafana instance**:
The Grafana server started for a Session, provisioned with the SAR data source and the Dashboard, and removed when the Session ends.
_Avoid_: viewer (Grafana's role for a read-only user, and the person opening dashboards), dashboard container

**Data source**:
The connection the Grafana instance reads its data through; sarDa provisions one, named SAR, pointing at the Sarda API. Grafana's JSON and provisioning files spell it `datasource`.
_Avoid_: datasource (in prose), backend

**Sarda API**:
The Prometheus-compatible HTTP API served by the executable on loopback, from which the Grafana instance reads the Series.
_Avoid_: exporter, metrics endpoint

**Parser container**:
The throwaway container, built from the sysstat image, that runs `sadf` against the SA files.
_Avoid_: sadf container, converter

**Runtime**:
The container engine found on the host, either `podman` or `docker`.
_Avoid_: engine, docker (when podman is meant)

**CSV export**:
The optional per-Metric CSV files written with `--export-csv`, kept for workflows that predate the Grafana instance.
_Avoid_: output dir, dump

## Dashboard layout

**Dashboard**:
The single page the Grafana instance opens: Rows of Panels for the data of one Session, preset to the Observation window.
_Avoid_: view, board, page

**Row**:
A collapsible band of Panels for one group of Activities (CPU, Disks, Network devices, ...). Grafana's other layout unit is the tab, which sarDa does not use.
_Avoid_: section, category

**Panel**:
One Visualization inside a Row, querying one Metric or a few related Metrics of the same Activity. Every Metric recorded in the SA files gets a Panel, even when all its Samples are zero; an Activity the files do not contain gets none.
_Avoid_: chart, graph, widget

**Visualization**:
What a Panel draws: the graphical representation of its query results; every sarDa Panel uses the time-series Visualization.
_Avoid_: chart, graph

**Variable**:
A Dashboard placeholder (host, cpu, dev, iface, ...) whose values come from the labels in the data, shown as a multi-select drop-down at the top of the page. Grafana's *filters* are a separate variable type, which sarDa does not use.
_Avoid_: filter, template variable, dropdown
