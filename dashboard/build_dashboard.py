#!/usr/bin/env python3

import json
from pathlib import Path

TEMPLATE_DIR = Path(__file__).parent / "templates"
HEADER_FILE = TEMPLATE_DIR / "header.json"
PANELS_FILE = TEMPLATE_DIR / "panels.json"
OUTPUT_FILE = Path(__file__).parent / "sar-csv.json"


def build_dashboard() -> None:
    header = json.loads(HEADER_FILE.read_text(encoding="utf-8"))
    panels = json.loads(PANELS_FILE.read_text(encoding="utf-8"))
    dashboard = {**header, "panels": panels}
    OUTPUT_FILE.write_text(json.dumps(dashboard, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    build_dashboard()
