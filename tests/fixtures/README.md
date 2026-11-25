# Test fixtures

* `sa/` - two real SA files recorded inside the parser image with `sadc -S XALL 1 5` / `sadc -S XALL 1 4` (sysstat 12.7.9; the hostname is the container ID). They must stay readable by the image's sysstat: sarDa refuses files written by a newer one.
* `sadf.stream` - the output of the exact script sarda runs in the parser container (`sarda --print-sadf-script`), executed inside the parser image with those two files, in order. The first two arguments are the skip bounds (empty: skip nothing). Regenerate with:

  ```sh
  cargo run -q -- --print-sadf-script > /tmp/script.sh
  podman run --rm -v "$PWD/tests/fixtures/sa:/in:ro" -v /tmp/script.sh:/s.sh:ro \
      localhost/sarda-sysstat:latest sh /s.sh "" "" /in/sa20261125 /in/sa20261126 \
      > tests/fixtures/sadf.stream
  ```

  Both files were recorded on 2026-11-25, which is why the second one (`sa20261126`) reports `file-date` 2026-11-25.

* `golden/` - the CSV files the original Python tool (`bin/sar-parser.py`, commit 82120a1, pandas 3.1.1) produced from the same two SA files, using the same 12.7.9 `sadf` as the image (a `sadf` wrapper that ran it in the container). `tests/golden.rs` requires the Rust `--export-csv` output to match them: raw files exactly, pivoted files numerically.

Change `golden/` only on purpose. After an intended change to the CSV export format, regenerate it with `sarda --no-grafana --export-csv tests/fixtures/golden tests/fixtures/sa/sa20261125 tests/fixtures/sa/sa20261126`, review the diff, and commit it with the code change. The legacy tool is retired, so the golden files are now held to the previous export, not to Python.
