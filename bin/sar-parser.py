import shutil
import subprocess
import argparse
import sys
from pathlib import Path

def check_sadf_exists():
    if shutil.which("sadf") is None:
        print("Error: sadf binary not found.", file=sys.stderr)
        sys.exit(1)

def parse_sa1_file(source_file, output_dir, format_arg):
    options = {
        "-B": "A_PAGE", "-b": "A_IO", "-d": "A_DISK", "-F": "A_FS", "-H": "A_HUGE", 
        "-I SUM": "A_IRQ_SUM", "-I XALL": "A_IRQ_XALL", "-m": "A_PWR", "-n": "A_NET_ALL",
        "-q": "A_QUEUE", "-r": "A_MEMORY", "-S": "A_MEMSWAP", "-u": "A_CPU", "-v": "A_KTABLES", 
        "-W": "A_SWAP", "-w": "A_PCSW", "-y": "A_SERIAL"
    }
    
    base_filename = source_file.stem
    for option, label in options.items():
        output_file = output_dir / f"{base_filename}-{label}.{format_arg.lstrip('-')}"
        command = ["sadf", format_arg, "-t", str(source_file), "--", option]
        try:
            with open(output_file, "w") as f:
                subprocess.run(command, stdout=f, check=True)
        except subprocess.CalledProcessError as e:
            print(f"Error: Command '{' '.join(command)}' failed with exit code {e.returncode}", file=sys.stderr)

    base_filename = source_file.stem
        output_file = output_dir / f"{base_filename}-{label}.{format_arg.lstrip('-')}"
    for option, label in zip(options, labels):
        output_file = output_dir / f"{base_filename}-{label}.{format_arg[1:]}"
        command = ["sadf", format_arg, "-t", str(source_file), "--", option]
        try:
            with open(output_file, "w") as f:
                subprocess.run(command, stdout=f, check=True)
                if output_file.stat().st_size == 0:
                    output_file.unlink()
                    print(f"Warning: Empty file {output_file} removed.", file=sys.stderr)
        except subprocess.CalledProcessError as e:
            print(f"Error: Command '{' '.join(command)}' failed with exit code {e.returncode}", file=sys.stderr)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Parse sa1 binary file using sadf.")
    parser.add_argument("-s", "--source_file", required=True, type=Path, help="Path to the sa1 binary file")
    parser.add_argument("-f", "--format", required=True, choices=["CSV", "JSON", "XML", "PCP"], help="Output format (CSV, JSON, XML, PCP)")
    parser.add_argument("-d", "--output_dir", type=Path, default=Path("."), help="Directory to store the output files (default: current directory)")

    args = parser.parse_args()

    source_file = args.source_file
    format_option = args.format
    output_dir = args.output_dir

    format_args = {
        "CSV": "-d",
        "JSON": "-j",
        "XML": "-x",
        "PCP": "-l"
    }

    if format_option not in format_args:
        print("Error: Invalid format option. Choose from CSV, JSON, XML, or PCP.", file=sys.stderr)
        sys.exit(1)

    format_arg = format_args[format_option]

    if not source_file.is_file():
        print(f"Error: File {source_file} does not exist.", file=sys.stderr)
        sys.exit(1)

    if not output_dir.is_dir():
        print(f"Error: Directory {output_dir} does not exist.", file=sys.stderr)
        sys.exit(1)

    check_sadf_exists()
    parse_sa1_file(source_file, output_dir, format_arg)