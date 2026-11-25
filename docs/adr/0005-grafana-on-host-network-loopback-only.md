# Run the Grafana instance on the host network, bound to loopback

Grafana must reach the Sarda API served by the host process. Bridge networking would need `host.containers.internal` (podman) or `host-gateway` (docker), behave differently per rootless network backend, and require the API to listen on a non-loopback address, exposing SAR data to the LAN. Instead Grafana runs with `--network=host`, the Sarda API binds 127.0.0.1, and Grafana listens on 127.0.0.1. Anonymous Admin access is acceptable only because of this loopback restriction; it must never be combined with a non-loopback bind.

## Consequences

Linux only. Docker Desktop on macOS/Windows is unsupported.
