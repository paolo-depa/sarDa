# sarDa

sarDa (SAR Dashboard) provides:
- a Python CLI (`sar-parser`) to convert `sa` files into CSV datasets
- a Grafana dashboard (`dashboard/sar-csv.json`) for those CSV files

## Installation (pyproject / PEP 621)

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -e .
```

For development tools (lint/test/build executable):

```bash
pip install -e ".[dev]"
```

## Usage

```bash
sar-parser -o ./csv /path/to/sa20260101 /path/to/sa20260102
```

You can also call the compatibility wrapper:

```bash
python bin/sar-parser.py --help
```

## Development

Use `make` targets:

- `make install` - create venv and install project + dev dependencies
- `make lint` - run ruff checks
- `make format` - run ruff formatter
- `make test` - run pytest
- `make build-dashboard` - regenerate `dashboard/sar-csv.json` from templates
- `make build-exe` - build Linux executable with PyInstaller
- `make clean` - remove build/test artifacts

## Building executable

```bash
make build-exe
```

The executable is generated at `dist/sar-parser` (Ubuntu/Linux in CI). For other OS targets, build on the target OS runner/host.

## Dashboard templating

`dashboard/sar-csv.json` is generated from:
- `dashboard/templates/header.json`
- `dashboard/templates/panels.json`

Regenerate after template edits:

```bash
make build-dashboard
```

## CI and updates

- GitHub Actions runs lint/tests on pull requests and on pushes to `main`
- Dependabot updates Python dependencies (`pyproject.toml`) and GitHub Actions

## Security notes

- Do not commit secrets/tokens/API keys
- Keep environment-specific values out of versioned files
- This repository currently contains no hardcoded credentials; continue using templates/examples for local configuration
