# Run `sadf` in a container, with user-provided images

`sadf` is the only reliable reader of the binary SA format, and its output depends on the sysstat version. Rather than require sysstat on every host, `sadf` always runs in a Parser container built from a configurable sysstat image. Because the environment is air-gapped, nothing is installed or pulled at run time: the repository ships a Containerfile to build the sysstat image once on a connected machine, and the executable only checks that the Grafana and sysstat images exist locally, printing exact build/load instructions when they do not.

## Consequences

Each file is first probed with `sadf -j -H`, which also yields its host and day, so sarda can skip files outside the Observation window without reading their data. When the probe fails, `sadf -c` is tried once to convert a datafile in the old (9.1.6+) format, and only the converted file is used if it probes cleanly.

Timestamps come out in UTC (container timezone), so the Observation window is filtered in Rust on parsed timestamps rather than with `sadf -s/-e`. A file written by a newer sysstat than the image provides yields no data, sometimes without any error, so sarDa refuses such files before parsing (exit 8; see ADR 0007 for the version check and the image contents).
