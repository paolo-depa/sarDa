# This script parses multiple sa1 binary files using the sadf command.
# Each sa1 file is parsed once for each of the metrics defined in the aggregators dictionary.
# When a metric is gathered from all the sa1 files passed as arguments, it is aggregated in a specific format (see format_args dictionary) 
# and saved in the corresponding output directory.

import argparse
import json
import shutil
import subprocess
import sys
import pandas as pd
import os

from pathlib import Path
import xml.etree.ElementTree as ET

format_args = {
    "csv": "-d",
    "json": "-j",
    "xml": "-x"
}


#TODO pivot the CPU, DISK and NETWORK_DEV aggragators to cpus, disks and network_devslisted as columns rather then in line
aggregators = {
    "io": { "sar_param": "-b" },
    "paging": { "sar_param": "-B" },
    # "cpu": { "sar_param": "-C" },
    "disk": { 
                "sar_param": "-d", 
                "pivot": {
                            "index": ["timestamp"], 
                            "columns": ["DEV"],
                            "skip_columns": ["# hostname", "interval"]
                }
    },
    "filesystem": { "sar_param": "-F" },
    "hugepages": { "sar_param": "-H" },
    "interrupts": { "sar_param": "-I ALL",
                    "pivot": {
                                "index": ["timestamp"], 
                                "columns": ["INTR"],
                                "skip_columns": ["# hostname", "interval"]
                    }
    },
    "power": { "sar_param": "-m ALL" },
    # This would produce a single file including all the statistics
    # which use different metrics: hardly readable by grafana
    # "network": { "sar_param": "-n ALL" },
    "network_dev": { "sar_param": "-n DEV",
                    "pivot": {
                                "index": ["timestamp"], 
                                "columns": ["IFACE"],
                                "skip_columns": ["# hostname", "interval"]
                    }
    },
    "network_edev": { "sar_param": "-n EDEV",
                     "pivot": {
                                "index": ["timestamp"], 
                                "columns": ["IFACE"],
                                "skip_columns": ["# hostname", "interval"]
                    }
    },
    "network_fc": { "sar_param": "-n FC" },
    "network_icmp": { "sar_param": "-n ICMP" },
    "network_eicmp": { "sar_param": "-n EICMP" },
    "network_icmp6": { "sar_param": "-n ICMP6" },
    "network_eicmp6": { "sar_param": "-n EICMP6" },
    "network_ip": { "sar_param": "-n IP" },
    "network_eip": { "sar_param": "-n EIP" },
    "network_ip6": { "sar_param": "-n IP6" },
    "network_eip6": { "sar_param": "-n EIP6" },
    "network_nfs": { "sar_param": "-n NFS" },
    "network_nfsd": { "sar_param": "-n NFSD" },
    "network_sock": { "sar_param": "-n SOCK" },
    "network_sock6": { "sar_param": "-n SOCK6" },
    # Not working as expected
    # "network_soft": { "sar_param": "-n SOFT" },
    "network_tcp": { "sar_param": "-n TCP" },
    "network_etcp": { "sar_param": "-n ETCP" },
    "network_udp": { "sar_param": "-n UDP" },
    "network_udp6": { "sar_param": "-n UDP6" },
    "per_cpu": { "sar_param": "-P ALL",
                    "pivot": {
                                "index": ["timestamp"], 
                                "columns": ["CPU"],
                                "skip_columns": ["# hostname", "interval"]
                    }
    },
    "queue": { "sar_param": "-q ALL" },
    "memory": { "sar_param": "-r ALL" },
    "swap_util": { "sar_param": "-S" },
    # "cpu_util": { "sar_param": "-u ALL" },
    "inode": { "sar_param": "-v" },
    "swap": { "sar_param": "-W" },
    "task": { "sar_param": "-w" },
    "tty": { "sar_param": "-y" },
}

def pivot_csv(file_path, index, columns, skip_columns ):
    
    separator = ";"
    
    df = pd.read_csv(file_path, sep=separator)
    values = [col for col in df.columns if col not in (index + columns + skip_columns)]
    for value in values:

        try:
            pivot_df = df.pivot(index=index, columns=columns, values=value)
        except Exception as e:
            print(f"An error occurred while pivotting the CSV: {e}")
            return

        # Sanitize the output file name
        suffix="".join(c if c.isalnum() or c in ('_', '.', '-') else '_' for c in value)
        output_file = f"{os.path.splitext(file_path)[0]}_{suffix}.csv"
        try:
            pivot_df.to_csv(output_file, sep=separator)
        except Exception as e:
            print(f"An error occurred while writing the pivotted CSV: {e}")
            return
        else:
            print(f"{file_path} pivotted and saved to {output_file}")

def merge_json_objects(obj1, obj2):
    """
    Merges two JSON objects recursively.

    Args:
        obj1 (dict): The first JSON object.
        obj2 (dict): The second JSON object.

    Returns:
        dict: The merged JSON object.
    """
    for key, value in obj2.items():
        if key in obj1:
            if isinstance(obj1[key], dict) and isinstance(value, dict):
                merge_json_objects(obj1[key], value)
            elif isinstance(obj1[key], list) and isinstance(value, list):
                obj1[key].extend(value)
            else:
                obj1[key] = value
        else:
            obj1[key] = value
    
    return obj1

def merge_xml_trees(root1, root2):
    """
    Merges two XML trees by combining their nodes.

    Args:
        root1 (ET.Element): The root element of the first XML tree.
        root2 (ET.Element): The root element of the second XML tree.

    Returns:
        str: The merged XML content as a string.
    """

    def merge_nodes(node1, node2):
        for child2 in node2:
            found = False
            for child1 in node1:
                if child1.tag == child2.tag and child1.attrib == child2.attrib:
                    merge_nodes(child1, child2)
                    found = True
                    break
            if not found:
                node1.append(child2)

    merge_nodes(root1, root2)
    retval = ET.tostring(root1, encoding="unicode")
    return retval

def merge_contents(file_content, result_content, format):
    """
    Merges the content of the current file with the result from the sadf command based on the format aggregator.

    Args:
        file_content (str): The current content of the file being processed.
        result_content (str): The result of the sadf command execution.
        format (str): The aggregator's format (csv, json, xml) to determine how to merge the content.

    Returns:
        str: The merged content.
    """

    if not file_content:
        file_content = result_content
    else:
        match format:

            case "json":
                result_content = json.loads(result_content)
                file_content = json.loads(file_content)
                file_content = merge_json_objects(file_content, result_content)
                file_content = json.dumps(file_content)
            
            case "xml":

                root1 = ET.fromstring(file_content)
                root2 = ET.fromstring(result_content)
                file_content = merge_xml_trees(root1, root2)

            case _:
                result_content = result_content.splitlines()
                if not result_content[-1].endswith("\n"):
                    result_content[-1] += "\n"
                file_content += "\n".join(result_content[1:])

    return file_content


def parse_sa1_files(source_files, output_dir, format, timeout, verbose):

    format_arg = format_args[format]

    for label, aggregator in aggregators.items():
        if verbose:
            print(f"- Processing: {label}")
        file_content = None
        for source_file in source_files:
            if verbose:
                print(f"-- Parsing: {source_file}")
            command = ["sadf", format_arg, str(source_file), "--"] + aggregator["sar_param"].split()
            try:
                # Run the sadf command with specified parameters
                # - stdout=subprocess.PIPE: Capture standard output
                # - stderr=subprocess.PIPE if not verbose else None: Capture standard error if not in verbose mode, otherwise print to console
                # - check=True: Raise CalledProcessError if the command exits with a non-zero status
                # - timeout=timeout: Set the timeout for the command
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE if verbose else subprocess.DEVNULL, check=True, timeout=timeout,encoding="utf-8")
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
                if verbose:
                    print(f"Warning: Command '{' '.join(command)}' for file '{source_file}' failed with exit code {e.returncode}. Exception: {e}", file=sys.stderr)
                    if e.stdout and len(e.stdout) > 0:
                        file_content = merge_contents(file_content, e.stdout, format)
                continue
            
            if result.stdout and len(result.stdout) > 0:
                file_content = merge_contents(file_content, result.stdout, format)
                
            
        if file_content is not None:
            output_file = output_dir / f"{label}.{format}"
            with open(output_file, "w") as f:
                f.write(file_content)
        
            if format == "csv" and "pivot" in aggregator:
                pivot_csv(output_file, aggregator["pivot"]["index"], aggregator["pivot"]["columns"], aggregator["pivot"]["skip_columns"])

if __name__ == "__main__":

    if not shutil.which("sadf"):
        print("Error: sadf binary not found.", file=sys.stderr)
        sys.exit(1)

    parser = argparse.ArgumentParser(description="Parse sa1 binary files using sadf and convert them to various formats (csv, json, xml).")
    parser.add_argument("source_files", type=Path, nargs='+', help="Paths to the sa1 binary files")
    parser.add_argument("-f", "--format", required=True, choices=["csv", "json", "xml"], help="Output format (csv, json, xml)")
    parser.add_argument("-o", "--output_dir", type=Path, help="Directory to store the output files (default: a new directory named after the format in the current directory)")
    parser.add_argument("-t", "--timeout", type=int, default=60, help="Timeout for sadf command in seconds (default: 60)")
    parser.add_argument("-v", "--verbose", action="store_true", help="Enable verbose output")

    args = parser.parse_args()

    source_files = args.source_files
    format = args.format
    timeout = args.timeout
    verbose = args.verbose
    
    if format not in format_args:
        print("Error: Invalid format aggregator. Choose from csv, json, or xml.", file=sys.stderr)
        sys.exit(1)

    for source_file in source_files:
        if not source_file.is_file():
            print(f"Error: File {source_file} does not exist.", file=sys.stderr)
            sys.exit(1)

    output_dir = Path(format)
    if args.output_dir:
        output_dir = args.output_dir

    if not output_dir.is_dir():
        try:
            output_dir.mkdir(parents=True, exist_ok=True)
        except OSError as e:
            print(f"Error: Unable to create output directory '{output_dir}': {e.strerror}", file=sys.stderr)
            sys.exit(1)

    parse_sa1_files(source_files, output_dir, format, timeout, verbose)
