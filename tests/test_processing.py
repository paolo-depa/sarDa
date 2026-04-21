from pathlib import Path

from sarda.processing import merge_contents, pivot_data, validate_csv


def test_merge_contents_skips_duplicate_header() -> None:
    current = "timestamp;value\n2024-01-01T00:00:00;1\n"
    incoming = "timestamp;value\n2024-01-01T00:10:00;2\n"
    assert merge_contents(current, incoming) == (
        "timestamp;value\n2024-01-01T00:00:00;1\n2024-01-01T00:10:00;2\n"
    )


def test_validate_csv_filters_restart_rows() -> None:
    stream = validate_csv(
        data_io=iter(["timestamp;value\n", "RESTART\n", "2024-01-01T00:00:00;10\n"]),
        separator=";",
        label="test",
    )
    assert stream is not None
    assert stream.getvalue() == "timestamp;value\n2024-01-01T00:00:00;10\n"


def test_pivot_data_writes_pivot_file(tmp_path: Path) -> None:
    src = tmp_path / "metric.csv"
    src.write_text(
        "timestamp;DEV;value\n2024-01-01T00:00:00;dev1;1\n2024-01-01T00:00:00;dev2;2\n",
        encoding="utf-8",
    )

    ok = pivot_data(
        src,
        {
            "index": ["timestamp"],
            "columns": ["DEV"],
            "skip_columns": [],
        },
    )

    assert ok
    pivoted = tmp_path / "metric_value.csv"
    assert pivoted.is_file()
    content = pivoted.read_text(encoding="utf-8")
    assert "dev1" in content
    assert "dev2" in content
    assert "2024-01-01T00:00:00;1;2" in content
