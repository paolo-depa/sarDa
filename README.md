# SAR Parser

This script parses multiple `sa1` binary files using the `sadf` command. Each `sa1` file is parsed once for each of the metrics defined in the `options` dictionary. When a metric is gathered from all the `sa1` files passed as arguments, it is aggregated in a specific format (see `format_args` dictionary) and saved in the corresponding output directory.

## Requirements

- Python 3.10 or higher
- `sadf` command (part of the `sysstat` package)

## Installation

1. Ensure you have Python 3.10 or higher installed.
2. Install the [sysstat] (https://github.com/sysstat/sysstat) package to get the `sadf` command

## Usage

```sh
python sar-parser.py [-h] -s SOURCE_FILES [SOURCE_FILES ...] -f {csv,json,xml} [-o OUTPUT_DIR] [-t TIMEOUT] [-v]
```

### Arguments

- `-s`, `--source_files`: Paths to the `sa1` binary files (required).
- `-f`, `--format`: Output format (csv, json, xml) (required).
- `-o`, `--output_dir`: Directory to store the output files (default: a new directory named after the format in the current directory).
- `-t`, `--timeout`: Timeout for `sadf` command in seconds (default: 60).
- `-v`, `--verbose`: Enable verbose output.

### Example

```sh
python sar-parser.py -s /path/to/sa1file1 /path/to/sa1file2 -f csv -o /path/to/output -t 120 -v
```

