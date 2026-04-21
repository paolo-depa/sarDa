PYTHON ?= python3
VENV ?= .venv
VENV_BIN := $(VENV)/bin

.PHONY: install lint format test build-exe build-dashboard clean

install:
	$(PYTHON) -m venv $(VENV)
	$(VENV_BIN)/pip install --upgrade pip
	$(VENV_BIN)/pip install -e ".[dev]"

lint:
	$(VENV_BIN)/ruff check src tests bin

format:
	$(VENV_BIN)/ruff format src tests bin

test:
	$(VENV_BIN)/pytest

build-dashboard:
	$(VENV_BIN)/python dashboard/build_dashboard.py

build-exe: build-dashboard
	$(VENV_BIN)/pyinstaller --onefile --name sar-parser src/sarda/__main__.py

clean:
	rm -rf .pytest_cache .ruff_cache build dist *.egg-info src/*.egg-info src/sarda.egg-info
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
	find . -type f -name '*.pyc' -delete
