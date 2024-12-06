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

options = {
    "-b": "io",
    "-B": "paging",
#    "-C": "cpu",
    "-d": "disk",
    "-F": "filesystem",
    "-H": "hugepages",
    "-I ALL": "interrupts",
    "-m ALL": "power",
    "-n ALL": "network",
    "-P ALL": "per_cpu",
    "-q ALL": "queue",
    "-r ALL": "memory",
    "-S": "swap_util",
#    "-u ALL": "cpu_util",
    "-v": "inode",
    "-W": "swap",
    "-w": "task",
    "-y": "tty",
}

def merge_json_objects(obj1, obj2):
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

def merge_contents(file_content, result, format_option, verbose):

    result_content = result.stdout

    if not file_content:
        file_content = result_content
    else:
        match format_option:

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
                file_content += "\n".join(result_content[1:])

    return file_content


def parse_sa1_files(source_files, output_dir, format_option, timeout, verbose):

    format_arg = format_args[format_option]

    for option, label in options.items():
        file_content = None
        for source_file in source_files:
            command = ["sadf", format_arg, "-t", str(source_file), "--"] + option.split()
            try:
                # Run the sadf command with specified parameters
                # - stdout=subprocess.PIPE: Capture standard output
                # - stderr=subprocess.PIPE if not verbose else None: Capture standard error if not in verbose mode, otherwise print to console
                # - check=True: Raise CalledProcessError if the command exits with a non-zero status
                # - timeout=timeout: Set the timeout for the command
                result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE if verbose else subprocess.DEVNULL, check=True, timeout=timeout,encoding="utf-8")
            except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as e:
                if verbose:
                    print(f"Warning: Command '{' '.join(command)}' for file '{source_file}' failed with exit code {e.returncode}.", file=sys.stderr)
                continue
            
            if result.stdout:
                file_content = merge_contents(file_content, result, format_option, verbose)
            
        if file_content is not None:
            output_file = output_dir / f"{label}.{format_option}"
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
    format_option = args.format
    timeout = args.timeout
    verbose = args.verbose
    
    if format_option not in format_args:
        print("Error: Invalid format option. Choose from csv, json, or xml.", file=sys.stderr)
        sys.exit(1)

    for source_file in source_files:
        if not source_file.is_file():
            print(f"Error: File {source_file} does not exist.", file=sys.stderr)
            sys.exit(1)

    output_dir = Path(format_option)
    if args.output_dir:
        output_dir = args.output_dir

    if not output_dir.is_dir():
        try:
            output_dir.mkdir(parents=True)
        except OSError as e:
            print(f"Error: Unable to create output directory '{output_dir}': {e.strerror}", file=sys.stderr)
            sys.exit(1)

    parse_sa1_files(source_files, output_dir, format_option, timeout, verbose)
