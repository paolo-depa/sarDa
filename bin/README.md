# SAR Parser

This CLI parses one or more `sa` binary data files with `sadf`, aggregates metrics, and writes CSV files consumable by Grafana.

## Requirements

- Python 3.10+
- `sadf` command (`sysstat` package)

## Installation

From repository root:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install -e .
```

## Usage

```sh
sar-parser [-h] [-o OUTPUT_DIR] [-t TIMEOUT] [-v] [-s START_DATE] [-e END_DATE] source_files [source_files ...]
```

Compatibility launcher still exists:

```sh
python bin/sar-parser.py --help
```

## Example

```sh
sar-parser -o /tmp/csvs -v /path/to/sa1file1 /path/to/sa1file2
```
