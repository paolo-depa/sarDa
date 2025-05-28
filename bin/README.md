# SAR Parser

This script parses one or more `sa` (system activity) binary data files (typically named `saYYYYMMDD` where YYYY is the year, MM is the month, and DD is the day) using the `sadf` command.
It processes these files to extract various system metrics defined internally (e.g., CPU usage, memory, I/O, network statistics).
The extracted data for each metric is aggregated across all provided `sa` files, cleaned, optionally filtered by a specified time window, and then saved as a CSV file in an output directory.
For certain metrics, the script can also pivot the data to create more specialized CSV views.
After parsing, the data will be available in UTC time.

## Requirements

- Python 3.6 or higher
- `sadf` command (part of the `sysstat` package)
- Python libraries:
  - `pandas`
  - `pytz`
  - `python-dateutil`

## Installation

1. Ensure you have Python 3.6 or higher installed.
2. Install the `sysstat` package, which provides the `sadf` command.
3. Install the required Python libraries:
   ```sh
   pip install -r bin/requirements.txt
   ```

## Usage

```sh
sar-parser.py [-h] [-o OUTPUT_DIR] [-t TIMEOUT] [-v] [-s START_DATE] [-e END_DATE] source_files [source_files ...]
```

### Arguments

-  `-h`, `--help`
Show the help message and exit
-  `-o OUTPUT_DIR`, `--output_dir OUTPUT_DIR`
Output directory (default: ./csv) (default: None)
-  `-t TIMEOUT`, `--timeout TIMEOUT`
Timeout for each sadf command execution (seconds) (default: 60)
-  `-v`, `--verbose`
Enable debug logs. (default: False)
-  `-s START_DATE`, `--start-date START_DATE`
Start date for time filtering (YYYY-MM-DDTHH:mm:ss) in local timezone (default: None)
-  `-e END_DATE`, `--end-date END_DATE`
End date for time filtering (YYYY-MM-DDTHH:mm:ss) in local timezone (default: None)

### Example

```sh
python sar-parser.py -o /tmp/csvs -v /path/to/sa1file1 /path/to/sa1file2
```
