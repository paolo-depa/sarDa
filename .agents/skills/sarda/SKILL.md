---
name: sarda
description: Run sarda to inspect sysstat SA files (saYYYYMMDD, /var/log/sa) as a Grafana dashboard, export them to CSV, or query their metrics with PromQL. Use when the user has SA/sar data to look at, asks what a host's CPU, memory, disk or network did over time, or wants a sar dashboard.
---

# sarda

`sarda` turns SA files into a Grafana dashboard, CSV files, or a queryable Prometheus-compatible API, fully offline. It blocks until stopped, so start it in the background.

## Run it

1. Check it is ready: `sarda --help` and `podman info` (or `docker info`). Missing images are pulled; if that fails (exit 4) the message says to run `sarda prepare --save` on a connected host and `sarda load` here; relay those to the user instead of working around them.
2. Start in the background with `--json` and read events from stdout:

   ```sh
   sarda --json /var/log/sa/sa20261125 /var/log/sa/sa20261126 > /tmp/sarda.out 2> /tmp/sarda.err &
   ```

   Wait for the first stdout line. `{"event":"ready","url":...}` means Grafana is up: give the user `url`, and use `data.hosts`, `data.from`, `data.to` to describe what was loaded. `{"event":"error",...}` carries `kind`, `exit_code` and `message`.
3. Stop with SIGTERM (`kill <pid>`) when the user is done. Shutdown removes the container and temp files and emits `{"event":"stopped"}`.

## Pick the mode

- **Dashboard for the user**: the default.
- **Analyse the data yourself**: use `sarda api`. The `api_ready` event gives `api_url`; query it:

  ```sh
  curl -s "$API/api/v1/query" --data-urlencode 'query=avg(sarda_per_cpu_pct_idle{cpu="all"})'
  curl -s "$API/api/v1/query_range" --data-urlencode 'query=sarda_memory_pct_memused' \
       --data-urlencode start=<unix> --data-urlencode end=<unix> --data-urlencode step=60
  curl -s "$API/api/v1/label/__name__/values"
  ```

  Metrics are `sarda_<activity>_<column>` with labels `host` and a per-entity label (`cpu`, `dev`, `iface`, ...); `sarda catalog` lists every activity. PromQL is a subset: selectors, `sum/avg/min/max/count` with `by`/`without`, number arithmetic. Anything else returns an explicit error.
- **CSV only**: `sarda export-csv DIR <files>` exits when done (`exported` event).

## Narrow the data

`--start` / `--end` take RFC 3339; a time without an offset is UTC. SA files cover one day each, so pass every day in the range.

## Exit codes

`3` no container runtime · `4` image missing · `5` no usable data (check `--start/--end` and the offset; check the sysstat image is not older than the files) · `6` Grafana failed to start · `7` port unavailable (retry without `--port`) · `8` an SA file the image's sysstat cannot read (written by a newer sysstat, or corrupt; files that are not SA files are skipped with a warning, and a directory stands for the `saYYYYMMDD` files in it): use `--sadf-image` with a newer sysstat, or older files. Details and the full table are in the README.
