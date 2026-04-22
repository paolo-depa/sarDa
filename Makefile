PYTHON ?= python3
VENV ?= .venv
VENV_BIN := $(VENV)/bin
JSONNET ?= jsonnet
JSONNET_DIR := dashboard/jsonnet
JSONNET_VENDOR := dashboard/vendor

.PHONY: install install-jsonnet lint format test build-dashboard build-exe clean

install:
	$(PYTHON) -m venv $(VENV)
	$(VENV_BIN)/pip install --upgrade pip
	$(VENV_BIN)/pip install -e ".[dev]"

# Install the jsonnet CLI (requires Go ≥1.21 on PATH)
install-jsonnet:
	go install github.com/google/go-jsonnet/cmd/jsonnet@latest

lint:
	$(VENV_BIN)/ruff check src tests bin

format:
	$(VENV_BIN)/ruff format src tests bin

test:
	$(VENV_BIN)/pytest

# Render the Grafana dashboard JSON from its jsonnet source.
# Requires the jsonnet binary on PATH (run `make install-jsonnet` first).
build-dashboard:
	$(JSONNET) -J $(JSONNET_VENDOR) $(JSONNET_DIR)/main.jsonnet > dashboard/sar-csv.json

build-exe:
	$(VENV_BIN)/pyinstaller --onefile --name sar-parser src/sarda/__main__.py

clean:
	rm -rf .pytest_cache .ruff_cache build dist *.egg-info src/*.egg-info src/sarda.egg-info
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
	find . -type f -name '*.pyc' -delete
	rm -f dashboard/sar-csv.json
