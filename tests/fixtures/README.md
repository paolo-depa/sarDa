# Test fixtures

* `sa/` - two real SA files recorded with `sadc -S XALL 1 5` / `sadc -S XALL 1 4` (sysstat 12.8.0, one host).
* `sadf.stream` - the output of the exact script sarda runs in the parser container (`sarda --print-sadf-script`), executed on the host with those two files, in order. The first two arguments are the skip bounds (empty: skip nothing). Regenerate with:

  ```sh
  cargo run -q -- --print-sadf-script > /tmp/script.sh
  sh /tmp/script.sh "" "" tests/fixtures/sa/sa20261125 tests/fixtures/sa/sa20261126 > tests/fixtures/sadf.stream
  ```

  Both files were recorded on 2026-11-25, which is why the second one (`sa20261126`) reports `file-date` 2026-11-25.

* `golden/` - the CSV files the original Python tool (`bin/sar-parser.py`, commit 82120a1, pandas 3.1.1) produced from the same two SA files. `tests/golden.rs` requires the Rust `--export-csv` output to match them: raw files exactly, pivoted files numerically.
