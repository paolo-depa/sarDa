#!/usr/bin/python3.11

import argparse
import io
import logging
import re
import shutil
import subprocess
import sys
from pathlib import Path

from typing import Any, Dict, List, Optional, Tuple, TextIO, Union

import pandas as pd


# --- Constants ---
SADF_TIMEOUT_RC = -1  # Custom return code for sadf timeout
SADF_ERROR_RC = -2    # Custom return code for other sadf execution errors
TIMESTAMP_COL = "timestamp" # Standard column name for time filtering
TIMESTAMP_FORMAT = "%Y-%m-%dT%H:%M:%S"

# --- Global regexps ---
FILTER_REGEXPS = [re.compile(r"RESTART"), re.compile(r"^#")]


# --- Configuration ---

# Setup basic logging (level adjusted in main() based on args)
logging.basicConfig(format='%(levelname)s: %(message)s', stream=sys.stderr)

logger = logging.getLogger(__name__)
# sadf output format configuration ('-d' means CSV)
FORMAT_CONFIG = {"format": "csv", "sadf_arg": "-d", "separator": ";"}

# Metric definitions for sadf and optional pivoting configuration
# Keys: metric label (used for filename)
# 'sar_param': string of options passed to sadf via '--'
# 'pivot': optional dict configuring pandas pivoting ('index', 'columns', 'skip_columns')
aggregators: Dict[str, Dict[str, Any]] = {
    # ... (aggregator definitions remain the same) ...
    "io": {"sar_param": "-b"},
    "paging": {"sar_param": "-B"},
    "power_cpu": {"sar_param": "-m CPU"},
    "power_fan": {"sar_param": "-m FAN"},
    "power_freq": {"sar_param": "-m FREQ"},
    "power_in": {"sar_param": "-m IN"},
    "power_temp": {"sar_param": "-m TEMP"},
    "power_USB": {"sar_param": "-m USB"},
    "disk": {
        "sar_param": "-d",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["DEV"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "filesystem": {"sar_param": "-F"},
    "hugepages": {"sar_param": "-H"},
    "interrupts": {
        "sar_param": "-I ALL",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["INTR"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_dev": {
        "sar_param": "-n DEV",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["IFACE"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_edev": {
        "sar_param": "-n EDEV",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["IFACE"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_fc": {"sar_param": "-n FC"},
    "network_icmp": {"sar_param": "-n ICMP"},
    "network_eicmp": {"sar_param": "-n EICMP"},
    "network_icmp6": {"sar_param": "-n ICMP6"},
    "network_eicmp6": {"sar_param": "-n EICMP6"},
    "network_ip": {"sar_param": "-n IP"},
    "network_eip": {"sar_param": "-n EIP"},
    "network_ip6": {"sar_param": "-n IP6"},
    "network_eip6": {"sar_param": "-n EIP6"},
    "network_nfs": {"sar_param": "-n NFS"},
    "network_nfsd": {"sar_param": "-n NFSD"},
    "network_sock": {"sar_param": "-n SOCK"},
    "network_sock6": {"sar_param": "-n SOCK6"},
    "network_soft": {"sar_param": "-n SOFT"},
    "network_tcp": {"sar_param": "-n TCP"},
    "network_etcp": {"sar_param": "-n ETCP"},
    "network_udp": {"sar_param": "-n UDP"},
    "network_udp6": {"sar_param": "-n UDP6"},
    "per_cpu": {
        "sar_param": "-P ALL",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["CPU"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "queue": {"sar_param": "-q"},
    "memory": {"sar_param": "-r ALL"},
    "swap_util": {"sar_param": "-S"},
    # "cpu%": {"sar_param": "-u ALL"}, # Often redundant
    "inode": {"sar_param": "-v"},
    "swap": {"sar_param": "-W"},
    "task": {"sar_param": "-w"},
    "tty": {
        "sar_param": "-y",
        "pivot": {
            "index": [TIMESTAMP_COL],
            "columns": ["TTY"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
}
# --- End Configuration ---

def validate_csv(data_io: TextIO, separator: str, label: str) -> Optional[TextIO]:
    """
    Validates CSV data from a text I/O, handling common errors, bad lines, and applying time filters.
    Returns the validated/filtered data as a string.

    Args:
        data_io: Text I/O object containing CSV data.
        separator: CSV delimiter.
        label: Label identifying the data being handled (e.g., metric name).

    Returns:
        A TextIO object containing the validated and potentially filtered CSV data if successful and non-empty, otherwise None.
    """
    filtered_lines: List[str] = []
    timestamp_index: Optional[int] = None
    header_line_found = False

    for line in data_io:
        line = line.strip()
        if not line:
            continue

        # Handle header line
        if not header_line_found:
            header_line_found = True
            # Check if the header line is valid and contains the timestamp column
            headers = [h.strip() for h in line.split(separator)]
            if TIMESTAMP_COL in headers:
                timestamp_index = headers.index(TIMESTAMP_COL)
                filtered_lines.append(line)
            else:
                logger.error(f"'{TIMESTAMP_COL}' column not found in header for {label}. Cannot filter by time.")
                return None
            continue

        # Filter lines based on regexps
        if any(regex.search(line) for regex in FILTER_REGEXPS):
            continue


        filtered_lines.append(line)

    if not filtered_lines:
        logger.info(f"No valid data found for {label} after filtering.")
        return None

    return io.StringIO("\n".join(filtered_lines) + "\n")


def pivot_data(file_path: Path, pivot_config: Dict[str, List[str]]) -> bool:
    """
    Reads a cleaned CSV file and pivots data based on configuration.
    Generates new CSV files (<base>_<value_col_suffix>.csv) for each pivoted value column.

    Args:
        file_path: Path to the input CSV file (should be cleaned).
        pivot_config: Dictionary with 'index', 'columns', 'skip_columns' lists.

    Returns:
        True if pivoting was attempted and at least one pivoted file was saved successfully, False otherwise.
    """
    separator = FORMAT_CONFIG["separator"]
    index_cols = pivot_config["index"]
    column_cols = pivot_config["columns"]
    skip_cols = pivot_config.get("skip_columns", []) # Use .get for optional key

    try:
        df = pd.read_csv(file_path, sep=separator, on_bad_lines='warn', low_memory=False, engine='c')
    except pd.errors.EmptyDataError:
        logger.warning(f"Skipping pivot for empty file: {file_path}")
        return False
    except Exception as e:
        logger.error(f"Failed to read file for pivoting {file_path}: {e}")
        return False

    required_cols = index_cols + column_cols
    missing_cols = [col for col in required_cols if col not in df.columns]
    if missing_cols:
        logger.error(f"Missing columns required for pivot in {file_path}: {missing_cols}. Skipping pivot.")
        return False

    value_cols = [col for col in df.columns if col not in (required_cols + skip_cols)]

    if not value_cols:
        logger.debug(f"No value columns found for pivoting in {file_path}. Skipping pivot.")
        return False

    logging.info(f"Pivoting {len(value_cols)} value column(s) for {file_path}")

    base_output_name = file_path.stem
    pivoted_files_saved = False

    for value in value_cols:
        try:
            # Ensure the value column still exists (paranoid check)
            if value not in df.columns:
                logger.debug(f"Value column '{value}' unexpectedly missing after deduplication in {file_path}. Skipping.")
                continue

            # Perform the pivot operation
            pivot_df = df.pivot(index=index_cols, columns=column_cols, values=value)
            # fill with 0 if any value is missing (it avoids a glitch in UQL library when processing the data in grafana)
            pivot_df.fillna(0, inplace=True)

        except ValueError as e:
            # Common if duplicates remain somehow, or other index issues
            logger.error(f"Pivot failed for {file_path}, value '{value}': {e}. Check data integrity.")
            continue
        except KeyError as e:
             # If specified index/column names are wrong
             logger.error(f"Pivot failed for {file_path}, value '{value}' due to KeyError: {e}. Check config.")
             continue
        except Exception as e:
             # Catch unexpected errors during pivot
             logging.error(f"Error during pivot for {file_path}, value '{value}': {e}")
             continue

        suffix = "".join(c if c.isalnum() or c in ("_", ".", "-") else "_" for c in str(value))
        if not suffix: suffix = f"pivot_value_{value_cols.index(value)}"

        # Construct output path

        output_file = file_path.with_name(f"{base_output_name}_{suffix}.csv")
        try:
            pivot_df.to_csv(output_file, sep=separator)
            logger.debug(f"Saved pivoted data for '{value}' to {output_file}")
            pivoted_files_saved = True # Mark success if at least one file is saved
        except Exception as e:
            logger.error(f"Failed writing pivoted file {output_file}: {e}")

    return pivoted_files_saved


def merge_contents(current_content: Optional[str], new_content: str) -> Optional[str]:
    """
    Merges new sadf output string with existing content string, handling headers and newlines.
    Ensures only one header and proper newline handling.

    Args:
        current_content: The existing aggregated content string, or None.
        new_content: The new content chunk string from sadf.

    Returns:
        The merged content string, or None if inputs are invalid/empty.
    """

    if not new_content or not new_content.strip():
        return current_content

    new_lines = new_content.strip().splitlines()
    if not new_lines:
        return current_content

    if not current_content:
        full_new_content = "\n".join(new_lines) + "\n"
        return full_new_content
    else:
        if len(new_lines) > 1:
            if not current_content.endswith("\n"):
                current_content += "\n"
            data_to_add = "\n".join(new_lines[1:]) + "\n"
            return current_content + data_to_add
        else:
            # If new_content only has one line (likely just a header), don't add anything
            # This assumes the first chunk always had data if current_content is not None
            logger.debug("Skipping merge of single-line new content (likely header only).")
            return current_content


# Removed the separate write_csv function as its logic is now part of validate_csv's return


def run_sadf(source_file: Path, sadf_args: List[str], sar_params: str, timeout: int) -> Tuple[Optional[str], Optional[str], int]:
    """
    Runs a single sadf command and returns its output and status.

    Args:
        source_file: Path to the input sa binary file.
        sadf_args: List of base arguments for sadf (e.g., ['-d']).
        sar_params: String of sar options to pass via '--'.
        timeout: Command timeout in seconds.

    Returns:
        A tuple containing: (stdout string or None, stderr string or None, return code).
        Return code is from subprocess, or SADF_TIMEOUT_RC / SADF_ERROR_RC on error.
    """
    command = ["sadf"] + sadf_args + [str(source_file), "--"] + sar_params.split()
    logger.debug(f"Running command: {' '.join(command)}")

    try:
        result = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False, # We handle non-zero exit codes manually
            timeout=timeout,
            encoding="utf-8",
            errors='ignore'
        )
        return result.stdout, result.stderr, result.returncode

    except subprocess.TimeoutExpired as e:
        logger.error(f"Command timed out ({timeout}s) for {source_file}: {' '.join(command)}")
        # Return partial output if available, and specific return code
        return e.stdout, e.stderr, SADF_TIMEOUT_RC
    except Exception as e:
        # Catch other errors like command not found (though checked earlier)
        logger.error(f"Failed running sadf command for {source_file}: {e}")
        return None, str(e), SADF_ERROR_RC


def process_metric(
    label: str,
    config: Dict[str, Any],
    source_files: List[Path],
    output_dir: Path,
    sadf_base_args: List[str],
    timeout: int,
    ) -> None:
    """
    Processes a single metric: runs sadf across files, merges, validates, writes, pivots.

    Args:
        label: The metric label (used for filename).
        config: The configuration dictionary for this metric from `aggregators`.
        source_files: List of input sa file paths.
        output_dir: Directory to save output files.
        sadf_base_args: Base arguments for sadf command.
        timeout: Timeout for sadf commands.
    """

    logger.info(f"Processing metric: {label} ({config['sar_param']}) ")

    aggregated_content: Optional[str] = None
    processed_files_count = 0
    separator = FORMAT_CONFIG["separator"]

    for source_file in source_files:
        logging.debug(f"Reading source: {source_file} for metric {label} ({config['sar_param']}) ")

        # Run sadf and merge results
        stdout, stderr, returncode = run_sadf(
            source_file, sadf_base_args, config["sar_param"], timeout
        )

        # Log significant stderr messages
        if stderr:
            stderr_lower = stderr.lower()
            # Ignore common/expected messages
            ignore_stderr = ["end of file", "no data", "requested activities not available"]
            if not any(msg in stderr_lower for msg in ignore_stderr):
                logger.warning(f"Metric {label} ({config['sar_param']}): error processing from {source_file}: {stderr.strip()}.")

        # Validate stdout content
        if stdout:
            # Pass the potentially UTC-aware start/end datetimes
            validated_stream = validate_csv(io.StringIO(stdout), separator, label)

            if validated_stream:
                processed_files_count += 1
                aggregated_content = merge_contents(aggregated_content, validated_stream.getvalue())
                validated_stream.close()
            else:
                 logger.info(f"sadf output for {label} from {source_file} resulted in no data after validation/filtering. Skipping merge.")

    # Post-aggregation processing (Write, Pivot)
    if aggregated_content:
        output_file = output_dir / f"{label}.{FORMAT_CONFIG['format']}"
        # Validate the *aggregated* content - no time filtering needed here, just structure
        final_validated_content = validate_csv(io.StringIO(aggregated_content), separator, label)

        if final_validated_content:
            try:
                with open(output_file, 'w', encoding='utf-8') as f_out:
                    f_out.write(final_validated_content.getvalue())

                logger.debug(f"Successfully wrote aggregated and validated data for {label} to {output_file}")

                # Pivot if config exists
                if "pivot" in config:
                    logger.debug(f"Pivoting data for {label}")
                    pivot_data(output_file, config["pivot"])

            except IOError as e:
                 logger.error(f"Failed writing final aggregated file {output_file}: {e}")
            except Exception as e:
                 logger.error(f"Error during post-aggregation write/pivot for {label} ({output_file}): {e}")
        else: logger.warning(f"Aggregated content for metric '{label}' was invalid or empty after final validation. No output file generated.")
    else:
         logger.warning(f"No valid content found for metric '{label}'.")



def main() -> None:
    """Parses arguments, sets up logging, and orchestrates the processing."""
    # --- Check for sadf dependency ---
    if not shutil.which("sadf"):
        logger.critical("sadf binary not found. Please install the 'sysstat' package.")
        sys.exit(1)

    # --- Argument Parsing ---
    parser = argparse.ArgumentParser(
        description="Parse sa binary files using sadf, aggregate, clean, filter by time, convert to csv, and optionally pivot.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "source_files", type=Path, nargs="+", help="Paths to the sa binary files"
    )
    parser.add_argument(
        "-o", "--output_dir", type=Path,
        help=f"Output directory (default: ./{FORMAT_CONFIG['format']})"
    )
    parser.add_argument(
        "-t", "--timeout", type=int, default=60,
        help="Timeout for each sadf command execution (seconds)"
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", default=False,
        help="Enable debug logs."
    )

    args = parser.parse_args()


    # --- Setup Logging Level ---
    log_level = logging.INFO # Default
    if args.verbose: log_level = logging.DEBUG

    logger.setLevel(log_level)
    logger.debug("Logging level set to %s", logging.getLevelName(log_level))

    # --- Validate source files ---
    valid_source_files: List[Path] = []
    for source_file in args.source_files:
        if not source_file.is_file():
            # Log as warning, script continues with valid files
            logging.warning(f"Source file not found or not a file: {source_file}. Skipping.")
        else:
            valid_source_files.append(source_file)

    if not valid_source_files:
         # Log as critical and exit if no valid inputs remain
         logger.critical("No valid source files provided.")
         sys.exit(1)
    elif len(valid_source_files) < len(args.source_files):
        logging.info(f"Processing {len(valid_source_files)} valid source file(s) out of {len(args.source_files)}.")

    # --- Determine and Create Output Directory ---
    output_dir = args.output_dir if args.output_dir else Path.cwd() / FORMAT_CONFIG["format"]
    try:
        output_dir.mkdir(parents=True, exist_ok=True)
        logger.debug(f"Using output directory: {output_dir.resolve()}")
    except OSError as e:
        logger.critical(f"Unable to create output directory '{output_dir}': {e}")
        sys.exit(1)

    # --- Process Metrics ---
    sadf_base_args = [FORMAT_CONFIG["sadf_arg"]]

    # Iterate through defined metrics and process each
    for label, config in aggregators.items():
        process_metric(
            label, config, valid_source_files, output_dir,
            sadf_base_args, args.timeout
        )

    logger.info(f"Processing complete. Output files are in: {output_dir.resolve()}")


if __name__ == "__main__":
    main()
