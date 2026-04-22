import io
import logging
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, TextIO

import pandas as pd
from dateutil import parser

from .config import FILTER_REGEXPS, FORMAT_CONFIG, TIMESTAMP_COL
from .sadf import get_sar_file_time_window, run_sadf

logger = logging.getLogger(__name__)


def _get_local_tz():
    return datetime.now().astimezone().tzinfo or timezone.utc


def validate_csv(data_io: TextIO, separator: str, label: str) -> Optional[TextIO]:
    filtered_lines: List[str] = []
    header_line_found = False

    for line in data_io:
        line = line.strip()
        if not line:
            continue

        if not header_line_found:
            header_line_found = True
            headers = [h.strip() for h in line.split(separator)]
            if TIMESTAMP_COL in headers:
                filtered_lines.append(line)
            else:
                logger.error(
                    "'%s' column not found in header for %s. Cannot filter by time.",
                    TIMESTAMP_COL,
                    label,
                )
                return None
            continue

        if any(regex.search(line) for regex in FILTER_REGEXPS):
            continue

        filtered_lines.append(line)

    if not filtered_lines:
        logger.info("No valid data found for %s after filtering.", label)
        return None

    return io.StringIO("\n".join(filtered_lines) + "\n")


def pivot_data(file_path: Path, pivot_config: Dict[str, List[str]]) -> bool:
    separator = FORMAT_CONFIG["separator"]
    index_cols = pivot_config["index"]
    column_cols = pivot_config["columns"]
    skip_cols = pivot_config.get("skip_columns", [])

    try:
        df = pd.read_csv(
            file_path, sep=separator, on_bad_lines="warn", low_memory=False, engine="c"
        )
    except pd.errors.EmptyDataError:
        logger.warning("Skipping pivot for empty file: %s", file_path)
        return False
    except (OSError, pd.errors.ParserError, ValueError) as exc:
        logger.error("Failed to read file for pivoting %s: %s", file_path, exc)
        return False

    required_cols = index_cols + column_cols
    missing_cols = [col for col in required_cols if col not in df.columns]
    if missing_cols:
        logger.error(
            "Missing columns required for pivot in %s: %s. Skipping pivot.", file_path, missing_cols
        )
        return False

    value_cols = [col for col in df.columns if col not in (required_cols + skip_cols)]
    if not value_cols:
        logger.debug("No value columns found for pivoting in %s. Skipping pivot.", file_path)
        return False

    logger.info("Pivoting %s value column(s) for %s", len(value_cols), file_path)

    base_output_name = file_path.stem
    pivoted_files_saved = False

    for value in value_cols:
        try:
            if value not in df.columns:
                logger.debug(
                    "Value column '%s' unexpectedly missing after deduplication in %s. Skipping.",
                    value,
                    file_path,
                )
                continue

            pivot_df = df.pivot(index=index_cols, columns=column_cols, values=value)
            pivot_df = pivot_df.fillna(0)
        except (ValueError, KeyError) as exc:
            logger.error("Pivot failed for %s, value '%s': %s", file_path, value, exc)
            continue
        suffix = "".join(c if c.isalnum() or c in ("_", ".", "-") else "_" for c in str(value))
        if not suffix:
            suffix = f"pivot_value_{value_cols.index(value)}"

        output_file = file_path.with_name(f"{base_output_name}_{suffix}.csv")
        try:
            pivot_df.to_csv(output_file, sep=separator)
            logger.debug("Saved pivoted data for '%s' to %s", value, output_file)
            pivoted_files_saved = True
        except OSError as exc:
            logger.error("Failed writing pivoted file %s: %s", output_file, exc)

    return pivoted_files_saved


def merge_contents(current_content: Optional[str], new_content: str) -> Optional[str]:
    if not new_content or not new_content.strip():
        return current_content

    new_lines = new_content.strip().splitlines()
    if not new_lines:
        return current_content

    if not current_content:
        return "\n".join(new_lines) + "\n"

    if len(new_lines) > 1:
        if not current_content.endswith("\n"):
            current_content += "\n"
        return current_content + "\n".join(new_lines[1:]) + "\n"

    logger.debug("Skipping merge of single-line new content (likely header only).")
    return current_content


def process_metric(
    label: str,
    config: Dict[str, Any],
    source_files: List[Path],
    output_dir: Path,
    sadf_base_args: List[str],
    timeout: int,
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
) -> None:
    logger.info("Processing metric: %s (%s)", label, config["sar_param"])

    aggregated_content: Optional[str] = None
    separator = FORMAT_CONFIG["separator"]

    for source_file in source_files:
        logger.debug(
            "Reading source: %s for metric %s (%s)", source_file, label, config["sar_param"]
        )
        sadf_args = sadf_base_args.copy()

        if start_date or end_date:
            sadf_start_param = None
            sadf_end_param = None

            file_start_date, file_end_date = get_sar_file_time_window(source_file)
            file_start_date_obj = parser.parse(file_start_date).replace(tzinfo=timezone.utc)
            file_end_date_obj = parser.parse(file_end_date).replace(tzinfo=timezone.utc)
            local_tz = _get_local_tz()
            if start_date:
                start_date_obj = parser.parse(start_date)
                if start_date_obj.tzinfo is None:
                    start_date_obj = start_date_obj.replace(tzinfo=local_tz)
                start_date_utc = start_date_obj.astimezone(timezone.utc)

                if start_date_utc > file_end_date_obj:
                    continue
                if file_start_date_obj < start_date_utc < file_end_date_obj:
                    sadf_start_param = start_date_obj.strftime("%H:%M:%S")

            if end_date:
                end_date_obj = parser.parse(end_date)
                if end_date_obj.tzinfo is None:
                    end_date_obj = end_date_obj.replace(tzinfo=local_tz)
                end_date_utc = end_date_obj.astimezone(timezone.utc)
                if end_date_utc <= file_start_date_obj:
                    continue
                if file_start_date_obj < end_date_utc < file_end_date_obj:
                    sadf_end_param = end_date_obj.strftime("%H:%M:%S")

            if sadf_start_param:
                sadf_args.extend(["-s", sadf_start_param])
                if sadf_end_param:
                    sadf_end_time = datetime.strptime(sadf_end_param, "%H:%M:%S").time()
                    sadf_start_time = datetime.strptime(sadf_start_param, "%H:%M:%S").time()
                    if sadf_end_time <= sadf_start_time:
                        sadf_end_param = "23:59:59"
                else:
                    sadf_end_param = "23:59:59"
                sadf_args.extend(["-e", sadf_end_param])
            elif sadf_end_param:
                sadf_args.extend(["-s", "00:00:00", "-e", sadf_end_param])

        stdout, stderr, _ = run_sadf(source_file, sadf_args, config["sar_param"], timeout)

        if stderr:
            stderr_lower = stderr.lower()
            ignore_stderr = ["end of file", "no data", "requested activities not available"]
            if not any(msg in stderr_lower for msg in ignore_stderr):
                logger.warning(
                    "Metric %s (%s): error processing from %s: %s.",
                    label,
                    config["sar_param"],
                    source_file,
                    stderr.strip(),
                )

        if stdout:
            validated_stream = validate_csv(io.StringIO(stdout), separator, label)
            if validated_stream:
                aggregated_content = merge_contents(aggregated_content, validated_stream.getvalue())
                validated_stream.close()
            else:
                logger.info(
                    "sadf output for %s from %s resulted in no data after "
                    "validation/filtering. Skipping merge.",
                    label,
                    source_file,
                )

    if aggregated_content:
        output_file = output_dir / f"{label}.{FORMAT_CONFIG['format']}"
        final_validated_content = validate_csv(io.StringIO(aggregated_content), separator, label)

        if final_validated_content:
            try:
                output_file.write_text(final_validated_content.getvalue(), encoding="utf-8")
                logger.debug(
                    "Successfully wrote aggregated and validated data for %s to %s",
                    label,
                    output_file,
                )
                if "pivot" in config:
                    logger.debug("Pivoting data for %s", label)
                    pivot_data(output_file, config["pivot"])
            except OSError as exc:
                logger.error("Failed writing final aggregated file %s: %s", output_file, exc)
        else:
            logger.warning(
                "Aggregated content for metric '%s' was invalid or empty "
                "after final validation. No output file generated.",
                label,
            )
    else:
        logger.warning("No valid content found for metric '%s'.", label)
