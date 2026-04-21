import json
import logging
import subprocess
from pathlib import Path
from typing import List, Optional, Tuple

from .config import SADF_ERROR_RC, SADF_TIMEOUT_RC

logger = logging.getLogger(__name__)


def get_sar_file_time_window(sar_file: Path) -> Tuple[str, str]:
    if not sar_file.is_file():
        raise FileNotFoundError(f"Sar file not found: {sar_file}")

    try:
        result_start = subprocess.run(
            ["sadf", "-j", "-H", str(sar_file)],
            capture_output=True,
            text=True,
            check=True,
        )
        sar_file_metadata = json.loads(result_start.stdout)
        sar_file_date = sar_file_metadata["sysstat"]["hosts"][0]["file-date"]
        return f"{sar_file_date}T00:00:00", f"{sar_file_date}T23:59:59"
    except (subprocess.CalledProcessError, json.JSONDecodeError, KeyError) as exc:
        raise RuntimeError(f"Error processing sar file {sar_file}: {exc}") from exc


def run_sadf(
    source_file: Path,
    sadf_args: List[str],
    sar_params: str,
    timeout: int,
    start_date: Optional[str] = None,
    end_date: Optional[str] = None,
) -> Tuple[Optional[str], Optional[str], int]:
    if start_date:
        sadf_args.extend(["-s", start_date])
    if end_date:
        sadf_args.extend(["-e", end_date])

    command = ["sadf"] + sadf_args + [str(source_file), "--"] + sar_params.split()
    logger.debug("Running command: %s", " ".join(command))

    try:
        result = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout,
            encoding="utf-8",
            errors="ignore",
        )
        return result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired as exc:
        logger.error("Command timed out (%s s) for %s: %s", timeout, source_file, " ".join(command))
        stdout = (
            exc.stdout.decode("utf-8", errors="ignore")
            if isinstance(exc.stdout, bytes)
            else exc.stdout
        )
        stderr = (
            exc.stderr.decode("utf-8", errors="ignore")
            if isinstance(exc.stderr, bytes)
            else exc.stderr
        )
        return stdout, stderr, SADF_TIMEOUT_RC
    except (OSError, subprocess.SubprocessError) as exc:
        logger.error("Failed running sadf command for %s: %s", source_file, exc)
        return None, str(exc), SADF_ERROR_RC
