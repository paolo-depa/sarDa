# sarDa

<img src="logo.png" alt="sarDa" width="160" align="right">

**sarDa** (SAR Dashboard) turns sysstat `sa` files into an interactive Grafana dashboard with one command, on machines that have no internet access.

```sh
sarda /var/log/sa/sa20261125 /var/log/sa/sa20261126
# ...
# Grafana is ready. Open: http://127.0.0.1:3000/d/sarda/sarda?orgId=1&from=...&to=...
# Changes made in Grafana are not saved. Press Ctrl-C to stop and clean up.
```

It converts the files, starts a preconfigured Grafana container, loads your data into it and prints the URL. Ctrl-C removes the container and every temporary file.

## What you need

| Requirement | Why |
|---|---|
| `podman` or `docker` (detected automatically, podman first) | runs `sadf` and Grafana |
| a **sysstat image** (default `ghcr.io/paolo-depa/sarda-sysstat:0.0.1`) | provides `sadf` (SUSE BCI 16.1, sysstat 12.7.9) |
| a **Grafana image** (default `docker.io/grafana/grafana-oss:12.4.3`) | the dashboard server |
| Linux | Grafana uses host networking |

Nothing else: no Python, no sysstat on the host, no Grafana plugins, and no network access once the two images are present.

## Prepare the images (once)

If an image is missing, sarda pulls it (with the engine's progress on stderr) and carries on; `--no-pull` skips the attempt and stops with instructions instead. On a connected machine nothing needs preparing.

On an **air-gapped** machine, carry the images over from a connected one. The archive keeps the image names, so the defaults are found again after `load`:

```sh
# connected machine
sarda prepare --save sarda-images.tar

# air-gapped machine
sarda load sarda-images.tar
```

`sarda prepare` pulls the sysstat and Grafana images (without `--save` that is all it does). Each release publishes the sysstat image to the GitHub Container Registry, tagged with the version (the tag without its `v`) and `latest`. To build it yourself instead, use `sarda prepare --build --sadf-image localhost/sarda-sysstat:latest` (it builds from `sarda containerfile`, so it needs network access too) and pass the same `--sadf-image` when you run sarda.

The sysstat image is `registry.suse.com/bci/bci-base:16.1` with the sysstat package it delivers (12.7.9 today; about 125 MB). sarDa checks every SA file's own version header and **refuses files written by a newer sysstat**, because `sadf` returns no data at all for them instead of failing. To read such files, put an image with a newer sysstat behind `--sadf-image`.

Use other images with `--sadf-image` / `--grafana-image` (or `SARDA_SADF_IMAGE` / `SARDA_GRAFANA_IMAGE`), for example from an internal registry. The sysstat inside the sysstat image must be at least as new as the one that wrote your SA files; sarDa enforces this and exits with code 8 when it is not.

## Usage

```text
sarda [OPTIONS] <SA_FILE|DIR>...        open a dashboard (the default)
sarda export-csv [OPTIONS] <DIR> <SA_FILE|DIR>...
sarda api [OPTIONS] <SA_FILE|DIR>...
sarda prepare [--save FILE] [--build]   fetch the images
sarda load <FILE>                       load an archive written by prepare --save
sarda containerfile | catalog           print the sysstat Containerfile / the table below
```

A directory stands for the `saDD` / `saYYYYMMDD` files in it, so `sarda /var/log/sa` works. A path that is not a system activity file (such as the text report `sar20261125`) is skipped with a warning, which also makes `sarda /var/log/sa/*` safe; exit code 8 is only for when none is usable.

Options go after the subcommand name; `--json` and `--verbose` may also go before it.

| Option | Meaning |
|---|---|
| `--start TIME`, `--end TIME` | keep only this window. RFC 3339, e.g. `2026-11-25T08:00:00+01:00` (also the `+0100` form of `journalctl -o short-iso`); **no offset means UTC** |
| `--port PORT` | Grafana port on 127.0.0.1 (default 3000, or any free port if 3000 is taken; `0` = any free port) |
| `--open` | open the dashboard in the default browser (`xdg-open`) |
| `--runtime auto\|podman\|docker` | container runtime (default `auto`) |
| `--grafana-image IMAGE`, `--sadf-image IMAGE` | override the images |
| `--sadf-timeout SECONDS` | give up when the conversion prints nothing for this long (default 120); a long conversion that keeps making progress is never cut off |
| `--no-pull` | never try to pull a missing image |
| `--json` | machine-readable events on stdout (see below) |
| `-v`, `--verbose` | debug details on stderr |

Examples:

```sh
# a single day, only the morning (UTC), on a fixed port
sarda --start 2026-11-25T06:00:00Z --end 2026-11-25T12:00:00Z --port 3100 /var/log/sa/sa20261125

# CSV only, no containers besides the conversion
sarda export-csv ./csv /var/log/sa/sa2026112*

# use your own Grafana or Prometheus-compatible client
sarda api /var/log/sa/sa20261125     # prints http://127.0.0.1:<port>
curl 'http://127.0.0.1:<port>/api/v1/query?query=avg(sarda_per_cpu_pct_idle)'
```

stdout carries only the result (the URL, or JSON events with `--json`); progress and warnings go to stderr.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | success, or clean shutdown after Ctrl-C / SIGTERM |
| 1 | unexpected error |
| 2 | invalid command line (no files, bad time, `--start` after `--end`) |
| 3 | no working container runtime |
| 4 | a required image is missing |
| 5 | no usable data in the SA files or in the `--start`/`--end` window |
| 6 | Grafana failed to start or cannot reach the data |
| 7 | a network port is unavailable |
| 8 | an SA file cannot be read by the image's sysstat (written by a newer sysstat, or corrupt), or none of the given files is a system activity file |

### JSON events (`--json`)

One JSON object per line on stdout:

| `event` | When | Main fields |
|---|---|---|
| `ready` | Grafana is up | `url`, `grafana_port`, `api_url`, `container`, `runtime`, `data` |
| `api_ready` | `sarda api`: the API is up | `api_url`, `data` |
| `exported` | `sarda export-csv` finished | `dir`, `files` |
| `stopped` | clean shutdown | |
| `error` | failure | `kind`, `exit_code`, `message` |

`data` has `hosts`, `from`, `to`, `series`, `metrics`, `activities` and `warnings`.

## How it works

1. Each file's sysstat version is read from its magic header and compared with the image's `sadf`; a file written by a newer sysstat, or one `sadf` cannot read, aborts the run with exit 8 (an empty dashboard would be misleading). Then one throwaway container (the sysstat image, no network) reads each file's header with `sadf -j -H`; a datafile in the old binary format is converted in place with `sadf -c`; a file whose day (from the header, not the file name) cannot overlap `--start`/`--end` is skipped without reading its data. Everything else is converted activity by activity.
2. sarda merges the files, drops `RESTART` markers and bad rows, and keeps the requested window (all UTC).
3. It serves the result on `127.0.0.1` through a small Prometheus-compatible API (the **Sarda API**).
4. It starts Grafana with `--network=host` on `127.0.0.1`, provisioned with a data source pointing at that API and a dashboard generated from what your files actually contain. Anonymous Admin access is enabled, which is safe only because everything listens on loopback.

The dashboard follows what was **collected**, not the values: every activity and metric recorded in your files gets its row and panels — an idle disk or a host that never swapped still shows a flat chart — while an activity your files do not contain gets neither. Per-CPU, per-disk, per-interface, ... panels use dashboard variables, so any host works. Rows below the first three start collapsed.

### Metrics and labels

Every SA column becomes a metric `sarda_<activity>_<column>` (`%util` → `pct_util`, `rkB/s` → `rkB_s`) with labels `host` and, for per-entity activities, the dimension label below. CPU `-1` (all CPUs) is labelled `all`.

### Query language

The Sarda API understands a subset of PromQL: selectors with `=`, `!=`, `=~`, `!~`; `sum`, `avg`, `min`, `max`, `count` with `by (...)` / `without (...)`; and arithmetic on numbers. Anything else (`rate()`, `topk`, range vectors, operators between series) fails with an explicit error. SA values are already rates or gauges, so nothing needs `rate()`. A sample stays valid for the larger of 5 minutes and twice the sampling interval.

### Supported activities

<!-- catalog:start -->
| Dashboard row | Activity | `sadf` options | Dimension label | Metric prefix |
|---|---|---|---|---|
| IO | Overall I/O transfer rates | `-b` | - | `sarda_io_` |
| Paging | Paging statistics | `-B` | - | `sarda_paging_` |
| Power | CPU instantaneous clock frequency | `-m CPU` | `cpu` | `sarda_power_cpu_` |
| Power | Fan speed | `-m FAN` | `fan` | `sarda_power_fan_` |
| Power | CPU average clock frequency | `-m FREQ` | `cpu` | `sarda_power_freq_` |
| Power | Voltage inputs | `-m IN` | `input` | `sarda_power_in_` |
| Power | Device temperatures | `-m TEMP` | `sensor` | `sarda_power_temp_` |
| Power | USB devices power | `-m USB` | `bus` | `sarda_power_USB_` |
| Disks | Block device activity | `-d` | `dev` | `sarda_disk_` |
| Filesystems | Mounted filesystem usage | `-F` | `filesystem` | `sarda_filesystem_` |
| Hugepages | Hugepages utilization | `-H` | - | `sarda_hugepages_` |
| Interrupts | Interrupts per second, per interrupt source | `-I ALL` | `intr` | `sarda_interrupts_` |
| Network devices | Network interface traffic | `-n DEV` | `iface` | `sarda_network_dev_` |
| Network devices | Network interface errors | `-n EDEV` | `iface` | `sarda_network_edev_` |
| Network FC | Fibre Channel traffic | `-n FC` | `fchost` | `sarda_network_fc_` |
| Network ICMP | IPv4 ICMP traffic | `-n ICMP` | - | `sarda_network_icmp_` |
| Network ICMP | IPv4 ICMP errors | `-n EICMP` | - | `sarda_network_eicmp_` |
| Network ICMP | IPv6 ICMP traffic | `-n ICMP6` | - | `sarda_network_icmp6_` |
| Network ICMP | IPv6 ICMP errors | `-n EICMP6` | - | `sarda_network_eicmp6_` |
| Network IP | IPv4 traffic | `-n IP` | - | `sarda_network_ip_` |
| Network IP | IPv4 errors | `-n EIP` | - | `sarda_network_eip_` |
| Network IP | IPv6 traffic | `-n IP6` | - | `sarda_network_ip6_` |
| Network IP | IPv6 errors | `-n EIP6` | - | `sarda_network_eip6_` |
| Network NFS | NFS client activity | `-n NFS` | - | `sarda_network_nfs_` |
| Network NFS | NFS server activity | `-n NFSD` | - | `sarda_network_nfsd_` |
| Network TCP/UDP | IPv4 socket usage | `-n SOCK` | - | `sarda_network_sock_` |
| Network TCP/UDP | IPv6 socket usage | `-n SOCK6` | - | `sarda_network_sock6_` |
| Network software processing | Software-based network processing, per CPU | `-n SOFT` | `cpu` | `sarda_network_soft_` |
| Network TCP/UDP | TCP traffic | `-n TCP` | - | `sarda_network_tcp_` |
| Network TCP/UDP | TCP errors | `-n ETCP` | - | `sarda_network_etcp_` |
| Network TCP/UDP | IPv4 UDP traffic | `-n UDP` | - | `sarda_network_udp_` |
| Network TCP/UDP | IPv6 UDP traffic | `-n UDP6` | - | `sarda_network_udp6_` |
| CPU | CPU utilization, per CPU | `-P ALL` | `cpu` | `sarda_per_cpu_` |
| Queue and load | Run queue length and load average | `-q` | - | `sarda_queue_` |
| Memory | Memory utilization | `-r ALL` | - | `sarda_memory_` |
| Swap | Swap space utilization | `-S` | - | `sarda_swap_util_` |
| Inodes | Inode, file and pseudo-terminal tables | `-v` | - | `sarda_inode_` |
| Swap | Swap paging | `-W` | - | `sarda_swap_` |
| Task | Context switches and process creation | `-w` | - | `sarda_task_` |
| TTY | Serial line activity | `-y` | `tty` | `sarda_tty_` |
<!-- catalog:end -->

## Troubleshooting

| Symptom | Fix |
|---|---|
| a whole dashboard row is missing | the host did not collect that activity. `sadc` collects only some by default: add `SADC_OPTIONS="-S XALL"` (or the keywords you need, see `man sadc`) to `/etc/sysconfig/sysstat`, restart `sysstat-collect.timer`, and collect again |
| a panel is flat at zero | the activity was collected but that device or counter did nothing in the window; the data is complete |
| `no working container runtime` | `podman info` (or `docker info`) must succeed for your user |
| `image ... is not available locally` | follow the printed instructions; names are overridable with the options above |
| `was written by sysstat X, but the sysstat image provides Y` | the files are newer than the image's sysstat: use `--sadf-image` with an image built on a newer sysstat. Datafiles in the old format are converted automatically, but files older than sysstat 9.1.6 cannot be |
| `no usable data` with `--start/--end` | times without an offset are **UTC**; add one, e.g. `+01:00` |
| Grafana cannot reach the data | something blocks loopback connections; check `-v` output and local firewall/proxy settings |
| port 3000 busy | sarda picks a free port automatically; pass `--port` to choose |
| leftover containers after a crash | the next run removes stale `sarda-<pid>-sadf` / `sarda-<pid>-grafana` containers whose process is gone |

## Building

```sh
cargo build --release          # target/release/sarda
cargo test                     # hermetic: uses fake container runtimes
```

Offline builds: `scripts/dist.sh` writes `dist/sarda-vendor-<version>.tar.xz`; extract it at the repository root of the offline machine and run `cargo build --release --offline`. See [AGENTS.md](AGENTS.md) for development notes, [GLOSSARY.md](GLOSSARY.md) for the vocabulary and [docs/adr](docs/adr) for design decisions.

## Curiosity

If you are just curious of what a SARDA is in italian language, have a look at this [Wikipedia page](https://it.wikipedia.org/wiki/Sarda_\(zoologia\)).
