import shutil
import subprocess
import argparse
import sys
from pathlib import Path

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

def check_sadf_exists():
    if not shutil.which("sadf"):
        print("Error: sadf binary not found.", file=sys.stderr)
        sys.exit(1)
    

def parse_sa1_files(source_files, output_dir, format_option, timeout):

    format_arg = format_args[format_option]

    for option, label in options.items():
        output_file = output_dir / f"{label}.{format_option}"
        with open(output_file, "a") as f:
            for source_file in source_files:
                command = ["sadf", format_arg, "-t", str(source_file), "--"] + option.split()
                try:
                    subprocess.run(command, stdout=f, check=True, timeout=timeout)
                except subprocess.CalledProcessError as e:
                    print(f"Warning: Command '{' '.join(command)}' for file '{source_file}' failed with exit code {e.returncode}.", file=sys.stderr)
                    continue
        if output_file.stat().st_size == 0:
            print(f"Warning: Output file '{output_file}' is empty and will be removed.", file=sys.stderr)
            output_file.unlink()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Parse sa1 binary files using sadf and convert them to various formats (CSV, JSON, XML).")
    parser.add_argument("source_files", type=Path, nargs='+', help="Paths to the sa1 binary files")
    parser.add_argument("-f", "--format", required=True, choices=["csv", "json", "xml"], help="Output format (csv, json, xml)")
    parser.add_argument("-o", "--output_dir", type=Path, default=Path.cwd(), help="Directory to store the output files (default: current directory)")
    parser.add_argument("-t", "--timeout", type=int, default=60, help="Timeout for sadf command in seconds (default: 60)")

    args = parser.parse_args()

    source_files = args.source_files
    format_option = args.format
    output_dir = args.output_dir
    timeout = args.timeout

    if format_option not in format_args:
        print("Error: Invalid format option. Choose from CSV, JSON, or XML.", file=sys.stderr)
        sys.exit(1)

    for source_file in source_files:
        if not source_file.is_file():
            print(f"Error: File {source_file} does not exist.", file=sys.stderr)
            sys.exit(1)

    if not output_dir.is_dir():
        print(f"Error: Directory {output_dir} does not exist.", file=sys.stderr)
        sys.exit(1)

    check_sadf_exists()
    parse_sa1_files(source_files, output_dir, format_option, timeout)