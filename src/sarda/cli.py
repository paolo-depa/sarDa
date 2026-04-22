import argparse
import logging
import shutil
import sys
from pathlib import Path

from .config import AGGREGATORS, FORMAT_CONFIG
from .processing import process_metric

logger = logging.getLogger(__name__)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Parse sa binary files using sadf, aggregate, clean, filter by time, "
            "convert to csv, and optionally pivot."
        ),
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("source_files", type=Path, nargs="+", help="Paths to the sa binary files")
    parser.add_argument(
        "-o",
        "--output_dir",
        type=Path,
        help=f"Output directory (default: ./{FORMAT_CONFIG['format']})",
    )
    parser.add_argument(
        "-t",
        "--timeout",
        type=int,
        default=60,
        help="Timeout for each sadf command execution (seconds)",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", default=False, help="Enable debug logs."
    )
    parser.add_argument(
        "-s",
        "--start-date",
        type=str,
        help="Start date for time filtering (YYYY-MM-DDTHH:mm:ss) in local timezone",
    )
    parser.add_argument(
        "-e",
        "--end-date",
        type=str,
        help="End date for time filtering (YYYY-MM-DDTHH:mm:ss) in local timezone",
    )
    return parser


def main() -> None:
    logging.basicConfig(format="%(levelname)s: %(message)s", stream=sys.stderr)

    if not shutil.which("sadf"):
        logger.critical("sadf binary not found. Please install the 'sysstat' package.")
        sys.exit(1)

    args = build_parser().parse_args()

    logger.setLevel(logging.DEBUG if args.verbose else logging.INFO)

    valid_source_files = [source_file for source_file in args.source_files if source_file.is_file()]
    for source_file in args.source_files:
        if not source_file.is_file():
            logger.warning("Source file not found or not a file: %s. Skipping.", source_file)

    if not valid_source_files:
        logger.critical("No valid source files provided.")
        sys.exit(1)

    output_dir = args.output_dir if args.output_dir else Path.cwd() / FORMAT_CONFIG["format"]
    try:
        output_dir.mkdir(parents=True, exist_ok=True)
    except OSError as exc:
        logger.critical("Unable to create output directory '%s': %s", output_dir, exc)
        sys.exit(1)

    sadf_base_args = [FORMAT_CONFIG["sadf_arg"]]
    for label, config in AGGREGATORS.items():
        process_metric(
            label,
            config,
            valid_source_files,
            output_dir,
            sadf_base_args,
            args.timeout,
            args.start_date,
            args.end_date,
        )

    logger.info("Processing complete. Output files are in: %s", output_dir.resolve())
