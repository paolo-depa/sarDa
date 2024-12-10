# This script parses multiple sa1 binary files using the sadf command.
# Each sa1 file is parsed once for each of the metrics defined in the aggregators dictionary.
# When a metric is gathered from all the sa1 files passed as arguments, it is aggregated in a specific format (see format_args dictionary) 
# and saved in the corresponding output directory.

import argparse
import json
import shutil
import subprocess
import sys

from pathlib import Path
import xml.etree.ElementTree as ET

format_args = {
    "csv": "-d",
    "json": "-j",
    "xml": "-x"
}


#TODO pivot the CPU, DISK and NETWORK_DEV aggragators to cpus, disks and network_devslisted as columns rather then in line
aggregators = {
    "-b": "io",
    "-B": "paging",
    # "-C": "cpu",
    "-d": "disk",
    "-F": "filesystem",
    "-H": "hugepages",
    "-I ALL": "interrupts",
    "-m ALL": "power",
    # This would produce a single file including all the statistics
    # which use different metrics: hardly readable by grafana
    # "-n ALL": "network",
    "-n DEV": "network_dev",
    "-n EDEV": "network_edev",
    "-n FC": "network_fc",
    "-n ICMP": "network_icmp",
    "-n EICMP": "network_eicmp",
    "-n ICMP6": "network_icmp6",
    "-n EICMP6": "network_eicmp6",
    "-n IP": "network_ip",
    "-n EIP": "network_eip",
    "-n IP6": "network_ip6",
    "-n EIP6": "network_eip6",
    "-n NFS": "network_nfs",
    "-n NFSD": "network_nfsd",
    "-n SOCK": "network_sock",
    "-n SOCK6": "network_sock6",
    # Not working as expected
    # "-n SOFT": "network_soft",
    "-n TCP": "network_tcp",
    "-n ETCP": "network_etcp",
    "-n UDP": "network_udp",
    "-n UDP6": "network_udp6",
    "-P ALL": "per_cpu",
    "-q ALL": "queue",
    "-r ALL": "memory",
    "-S": "swap_util",
    # "-u ALL": "cpu_util",
    "-v": "inode",
    "-W": "swap",
    "-w": "task",
    "-y": "tty",
}

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

def merge_contents(file_content, result, format):
    """
    Merges the content of the current file with the result from the sadf command based on the format aggregator.

    Args:
        file_content (str): The current content of the file being processed.
        result (subprocess.CompletedProcess): The result of the sadf command execution.
        format (str): The aggregator's format (csv, json, xml) to determine how to merge the content.

    Returns:
        str: The merged content.
    """

    result_content = result.stdout

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

    for aggregator, label in aggregators.items():
        if verbose:
            print(f"- Processing: {label}")
        file_content = None
        for source_file in source_files:
            if verbose:
                print(f"-- Parsing: {source_file}")
            command = ["sadf", format_arg, "-t", str(source_file), "--"] + aggregator.split()
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
                continue
            
            if result.stdout:
                file_content = merge_contents(file_content, result, format)
            
        if file_content is not None:
            output_file = output_dir / f"{label}.{format}"
            with open(output_file, "w") as f:
                f.write(file_content)

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
