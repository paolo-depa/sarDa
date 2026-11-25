# Build sysstat into a BCI micro image

The parser image used to be the openSUSE Tumbleweed base plus `zypper install sysstat` (138 MB). We wanted something lighter on a SUSE-supported base, but simply taking the sysstat from a SLE base image was not viable, and the reason is worth recording:

- SLE 15.7's sysstat 12.0.2 reads newer files, but silently drops the counters it does not know (discard I/O `dkB/s`, `dtps`, `bdscd/s`, NUMA `pgprom/s`/`pgdem/s`, `blg_len`, `kbshmem`, hugepage `kbhugrsvd`/`kbhugsurp`): 8 of 40 activities lost columns.
- SLE 16.0's sysstat 12.7.5 refuses a file written by 12.8 outright - no output, no error, exit code 0.
- Only a sysstat at least as new as the file writer reads everything, so the image should carry the newest release, which no SLE base image ships.

So the build stage compiles upstream sysstat (pinned `SYSSTAT_VERSION`, checksummed tarball, PCP/sensors/NLS/documentation disabled) on `registry.suse.com/bci/bci-base:16.0`, and the final image is `registry.suse.com/bci/bci-micro:16.0` plus the single `sadf` binary, which links only libc. The result is 20 MB.

## Consequences

- Building the image needs network access (zypper plus the sysstat tarball from GitHub, `SYSSTAT_URL` overridable for a mirror). The runtime host still needs nothing.
- A newer sysstat release than the image is built with gives no data, sometimes silently. Rebuild with `--build-arg SYSSTAT_VERSION=... --build-arg SYSSTAT_SHA256=...`.
- The parser script must keep working with the micro image's toolset: no `sed`, `grep` or `awk` (a unit test rejects them; `tests/script.rs` runs the script against a fake `sadf`).
