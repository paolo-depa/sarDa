# Use the sysstat package delivered by the SUSE base image

The parser image installs the sysstat package that `registry.suse.com/bci/bci-base:16.1` delivers (12.7.9 today) instead of compiling sysstat from upstream source. The image is therefore a supported base plus a distribution package, with security updates attached to it, at the cost of size (125 MB rather than the 20 MB a hand-assembled micro image reached) and of currency: an SA file written by a newer sysstat cannot be read.

The currency cost is why sarDa refuses such files explicitly. Measurements with the same SA files:

- sysstat 12.7.5 (SLE 16.0) and 12.7.9 (SLE 16.1) return **no data and exit 0** for files written by 12.8 - a silent empty dashboard.
- sysstat 12.0.2 (SLE 15.7) reads newer files but silently omits counters it does not know (discard I/O, NUMA paging, hugepage extras).

So before anything is parsed, sarDa reads each file's magic header (`sadf::read_sa_version`: sysstat magic, format magic, version bytes, either endianness) and compares it with the version reported by the image's `sadf` (carried in the conversion stream as `@@VERSION`). A newer file, or one `sadf` cannot read at all, aborts the run with exit code 8 naming both versions; files outside `--start`/`--end` are still skipped, as before.

## Consequences

- Tumbleweed-era files (12.8+) need an image built on a newer sysstat; `--sadf-image` exists for that and nothing in the code assumes the delivered version.
- The parser script stays within POSIX `sh` and coreutils (`sh`, `mktemp`, `tr`, `cat`, `rm`; a test rejects `sed`, `grep` and `awk`), so the image can move to a minimal base without touching the script.
- Do not detect incompatibility from empty output: `sadf` succeeds with zero rows, so only the header comparison catches it.
