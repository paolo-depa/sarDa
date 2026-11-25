# Run `sadf` in a container, with user-provided images

`sadf` is the only reliable reader of the binary SA format, and its output depends on the sysstat version. Rather than require sysstat on every host, `sadf` always runs in a Parser container built from a configurable sysstat image. Because the environment is air-gapped, nothing is installed or pulled at run time: the repository ships a Containerfile to build the sysstat image once on a connected machine, and the executable only checks that the Grafana and sysstat images exist locally, printing exact build/load instructions when they do not.

## Consequences

Timestamps come out in UTC (container timezone), so the Observation window is filtered in Rust on parsed timestamps rather than with `sadf -s/-e`. SA files written by a newer sysstat than the image provides may fail to parse; the error must name both versions.
