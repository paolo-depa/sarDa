# sarDa

sarDa (SAR Dashboard) provides:
- a Python CLI (`sar-parser`) to convert `sa` files into CSV datasets
- a Grafana dashboard built with [jsonnet](https://jsonnet.org/) + [grafonnet](https://github.com/grafana/grafonnet)

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
- `make install-jsonnet` - install the `jsonnet` CLI via Go
- `make lint` - run ruff checks
- `make format` - run ruff formatter
- `make test` - run pytest
- `make build-dashboard` - generate `dashboard/sar-csv.json` from jsonnet source
- `make build-exe` - build Linux executable with PyInstaller
- `make clean` - remove build/test artifacts

## Building executable

```bash
make build-exe
```

The executable is generated at `dist/sar-parser` (Ubuntu/Linux in CI). For other OS targets, build on the target OS runner/host.

## Dashboard (jsonnet + grafonnet)

`dashboard/sar-csv.json` is **generated** – do not edit it directly.

Edit the source instead:
- `dashboard/jsonnet/main.jsonnet` – rows, panels, and variables
- `dashboard/jsonnet/lib/panels.libsonnet` – reusable CSV panel/row helpers

Regenerate after edits:

```bash
make install-jsonnet   # once
make build-dashboard
```

See [dashboard/README.md](dashboard/README.md) for details.

## CI and updates

- GitHub Actions runs lint/tests and dashboard build on pull requests and on pushes to `main`
- Dependabot updates Python dependencies (`pyproject.toml`) and GitHub Actions

## Security notes

- Do not commit secrets/tokens/API keys
- Keep environment-specific values out of versioned files
- This repository currently contains no hardcoded credentials; continue using templates/examples for local configuration
