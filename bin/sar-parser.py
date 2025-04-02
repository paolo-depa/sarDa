#!/usr/bin/python3.11

import argparse
import shutil
import subprocess
import sys
import pandas as pd
import os

from pathlib import Path

FORMAT_CONFIG = {"format": "csv", "sadf_arg": "-d", "separator": ";"}

aggregators = {
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
            "index": ["timestamp"],
            "columns": ["DEV"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "filesystem": {"sar_param": "-F"},
    "hugepages": {"sar_param": "-H"},
    "interrupts": {
        "sar_param": "-I ALL",
        "pivot": {
            "index": ["timestamp"],
            "columns": ["INTR"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_dev": {
        "sar_param": "-n DEV",
        "pivot": {
            "index": ["timestamp"],
            "columns": ["IFACE"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "network_edev": {
        "sar_param": "-n EDEV",
        "pivot": {
            "index": ["timestamp"],
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
            "index": ["timestamp"],
            "columns": ["CPU"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
    "queue": {"sar_param": "-q ALL"},
    "memory": {"sar_param": "-r ALL"},
    "swap_util": {"sar_param": "-S"},
    # "cpu%": {"sar_param": "-u ALL"},
    "inode": {"sar_param": "-v"},
    "swap": {"sar_param": "-W"},
    "task": {"sar_param": "-w"},
    "tty": {
        "sar_param": "-y",
        "pivot": {
            "index": ["timestamp"],
            "columns": ["TTY"],
            "skip_columns": ["# hostname", "interval"],
        },
    },
}


def parse(file_path, pivot_data):
    """
    Parses and pivots a CSV file based on the provided pivot data.

    Args:
        file_path (str): The path to the CSV file.
        pivot_data (dict): A dictionary containing pivot configuration:
            - index (list): List of columns to use as index.
            - columns (list): List of columns to use as columns.
            - skip_columns (list): List of columns to skip.
    """
    separator = FORMAT_CONFIG["separator"]
    index = pivot_data["index"]
    columns = pivot_data["columns"]
    skip_columns = pivot_data["skip_columns"]

    try:
        df = pd.read_csv(file_path, sep=separator)
    except pd.errors.EmptyDataError:
        print(f"Warning: Empty CSV file: {file_path}")
        return
    except FileNotFoundError:
        print(f"Error: File not found: {file_path}")
        return
    except Exception as e:
        print(f"Error: An error occurred while reading the CSV file: {file_path} - {e}")
        return

    values = [
        col for col in df.columns if col not in (index + columns + skip_columns)
    ]
    for value in values:
        try:
            pivot_df = df.pivot(index=index, columns=columns, values=value)
        except ValueError as e:
            print(f"Warning: pivot failed for {file_path} with value {value}: {e}")
            continue

        suffix = "".join(
            c if c.isalnum() or c in ("_", ".", "-") else "_" for c in value
        )
        output_file = f"{os.path.splitext(file_path)[0]}_{suffix}.csv"
        pivot_df.to_csv(output_file, sep=separator)
        print(f"{file_path} pivoted and saved to {output_file}")


def merge_contents(file_content, result_content):
    """
    Merges the content of the current file with the result from the sadf command.

    Args:
        file_content (str): The existing content of the file.
        result_content (str): The content to merge from the sadf command.

    Returns:
        str: The merged content.
    """
    if not file_content:
        return result_content

    result_content = result_content.splitlines()
    if not result_content[-1].endswith("\n"):
        result_content[-1] += "\n"
    file_content += "\n".join(result_content[1:])

    return file_content


def parse_sa1_files(source_files, output_dir, timeout, verbose):
    """
    Parses sa1 files, aggregates the results, and optionally pivots the data.

    Args:
        source_files (list): List of paths to the sa1 binary files.
        output_dir (Path): Directory to store the output files.
        timeout (int): Timeout for sadf command in seconds.
        verbose (bool): Enable verbose output.
    """
    format_arg = FORMAT_CONFIG["sadf_arg"]
    format_name = FORMAT_CONFIG["format"]

    for label, aggregator in aggregators.items():
        if verbose:
            print(f"- Processing: {label}")
        file_content = None
        for source_file in source_files:
            if verbose:
                print(f"-- Parsing: {source_file}")
            command = (
                ["sadf", format_arg, str(source_file), "--"]
                + aggregator["sar_param"].split()
            )
            try:
                result = subprocess.run(
                    command,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE if verbose else subprocess.DEVNULL,
                    check=True,
                    timeout=timeout,
                    encoding="utf-8",
                )
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
                if verbose:
                    print(
                        f"Warning: Command '{' '.join(command)}' for file '{source_file}' failed with exit code {e.returncode}. Exception: {e}",
                        file=sys.stderr,
                    )
                    if e.stdout and len(e.stdout) > 0:
                        file_content = merge_contents(file_content, e.stdout)
                continue

            if result.stdout and len(result.stdout) > 0:
                file_content = merge_contents(file_content, result.stdout)

        if file_content is not None:
            output_file = output_dir / f"{label}.{format_name}"
            with open(output_file, "w") as f:
                f.write(file_content)

            if "pivot" in aggregator:
                parse(output_file, aggregator["pivot"])


if __name__ == "__main__":
    if not shutil.which("sadf"):
        print("Error: sadf binary not found.", file=sys.stderr)
        sys.exit(1)

    parser = argparse.ArgumentParser(
        description="Parse sa1 binary files using sadf and convert them to csv format."
    )
    parser.add_argument(
        "source_files", type=Path, nargs="+", help="Paths to the sa1 binary files"
    )
    parser.add_argument(
        "-o",
        "--output_dir",
        type=Path,
        help="Directory to store the output files (default: a new directory named 'csv' in the current directory)",
    )
    parser.add_argument(
        "-t",
        "--timeout",
        type=int,
        default=60,
        help="Timeout for sadf command in seconds (default: 60)",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Enable verbose output"
    )

    args = parser.parse_args()

    source_files = args.source_files
    timeout = args.timeout
    verbose = args.verbose

    for source_file in source_files:
        if not source_file.is_file():
            print(f"Error: File {source_file} does not exist.", file=sys.stderr)
            sys.exit(1)

    output_dir = Path(FORMAT_CONFIG["format"])
    if args.output_dir:
        output_dir = args.output_dir

    if not output_dir.is_dir():
        try:
            output_dir.mkdir(parents=True, exist_ok=True)
        except OSError as e:
            print(
                f"Error: Unable to create output directory '{output_dir}': {e.strerror}",
                file=sys.stderr,
            )
            sys.exit(1)

    parse_sa1_files(source_files, output_dir, timeout, verbose)
