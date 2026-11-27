//! `sarda export-csv`: the per-Activity CSV files the original Python tool wrote,
//! including one pivoted file per metric for Activities with a Dimension.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::catalog::TIMESTAMP_COLUMN;
use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::table::{Table, SEPARATOR};
use crate::timeutil;

/// File name suffix for a pivoted metric column: anything but letters,
/// digits, `_`, `.` and `-` becomes `_`.
fn file_suffix(column: &str) -> String {
    column
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn format_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Orders dimension values numerically when they all are integers (so CPU 2
/// sorts before CPU 10), lexicographically otherwise.
fn sort_dimension_values(values: &mut [String]) {
    if values.iter().all(|v| v.trim().parse::<i64>().is_ok()) {
        values.sort_by_key(|v| v.trim().parse::<i64>().unwrap_or(0));
    } else {
        values.sort();
    }
}

/// Pivoted CSV files for `table`: `(file name, content)` per metric. Rows are
/// timestamps, columns are the entities of the Dimension, and a missing or
/// non-finite value is `0.0`, as the legacy tool wrote it. A metric without a
/// single number gets no file.
pub fn pivot(table: &Table) -> Vec<(String, String)> {
    let layout = table.layout();
    if layout.dimension.is_none() {
        return Vec::new();
    }
    let entity = |r| table.dimension(r).unwrap_or_default();

    let mut dims: Vec<String> = table
        .records
        .iter()
        .map(|r| entity(r).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sort_dimension_values(&mut dims);
    let dim_pos: BTreeMap<&str, usize> = dims
        .iter()
        .enumerate()
        .map(|(i, d)| (d.as_str(), i))
        .collect();
    let stamps: BTreeSet<i64> = table.records.iter().map(|r| r.ts).collect();

    let sep = SEPARATOR.to_string();
    let mut files = Vec::new();
    for &c in &layout.metrics {
        let mut grid: BTreeMap<i64, Vec<f64>> =
            stamps.iter().map(|t| (*t, vec![0.0; dims.len()])).collect();
        let mut any = false;
        for r in &table.records {
            if let Some(v) = r.number(c) {
                any = true;
                grid.get_mut(&r.ts).unwrap()[dim_pos[entity(r)]] = v;
            }
        }
        if !any {
            continue;
        }
        let mut out = format!("{TIMESTAMP_COLUMN}{sep}{}\n", dims.join(&sep));
        for (t, vals) in &grid {
            out.push_str(&timeutil::format_sadf(*t));
            for v in vals {
                out.push(SEPARATOR);
                out.push_str(&format_value(*v));
            }
            out.push('\n');
        }
        let name = format!(
            "{}_{}.csv",
            table.activity.label,
            file_suffix(table.column_name(c))
        );
        files.push((name, out));
    }
    files
}

/// Writes every CSV file into `dir` (created if needed); returns how many.
pub fn export(ds: &Dataset, dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::other(format!("cannot create {}: {e}", dir.display())))?;
    let mut count = 0;
    let write = |name: &str, content: &str| -> Result<()> {
        std::fs::write(dir.join(name), content)
            .map_err(|e| Error::other(format!("writing {name}: {e}")))
    };
    for table in &ds.tables {
        write(&format!("{}.csv", table.activity.label), &table.to_csv())?;
        count += 1;
        if table.activity.csv_pivot {
            for (name, content) in pivot(table) {
                write(&name, &content)?;
                count += 1;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::activity;

    fn table(label: &str, text: &str) -> Table {
        Table::parse(text, activity(label).unwrap()).unwrap().0
    }

    fn cpu() -> Table {
        table(
            "per_cpu",
            "# hostname;interval;timestamp;CPU;%user;%idle\n\
             h;1;2026-11-25 10:00:00 UTC;-1;10.00;90.00\n\
             h;1;2026-11-25 10:00:00 UTC;10;5.00;95.00\n\
             h;1;2026-11-25 10:00:00 UTC;2;20.00;80.00\n\
             h;1;2026-11-25 10:00:01 UTC;2;21.50;78.50\n",
        )
    }

    fn file<'a>(files: &'a [(String, String)], name: &str) -> &'a str {
        &files.iter().find(|(n, _)| n == name).unwrap().1
    }

    #[test]
    fn pivot_orders_numeric_dimensions_and_zero_fills() {
        let files = pivot(&cpu());
        let lines: Vec<&str> = file(&files, "per_cpu__user.csv").lines().collect();
        assert_eq!(lines[0], "timestamp;-1;2;10");
        assert_eq!(lines[1], "2026-11-25 10:00:00 UTC;10.0;20.0;5.0");
        assert_eq!(lines[2], "2026-11-25 10:00:01 UTC;0.0;21.5;0.0");
    }

    #[test]
    fn non_finite_values_are_zero_like_in_the_legacy_files() {
        let t = table(
            "disk",
            "# hostname;interval;timestamp;DEV;tps;await\n\
             h;1;2026-11-25 10:00:00 UTC;sda;1.00;-nan\n\
             h;1;2026-11-25 10:00:00 UTC;sdb;2.00;4.00\n\
             h;1;2026-11-25 10:00:01 UTC;sda;3.00;inf\n\
             h;1;2026-11-25 10:00:01 UTC;sdb;4.00;6.00\n",
        );
        let files = pivot(&t);
        assert_eq!(
            file(&files, "disk_await.csv"),
            "timestamp;sda;sdb\n2026-11-25 10:00:00 UTC;0.0;4.0\n2026-11-25 10:00:01 UTC;0.0;6.0\n"
        );
        // The plain export is still sadf's text, -nan included.
        assert!(t.to_csv().contains("sda;1.00;-nan"));
    }

    #[test]
    fn a_metric_without_any_number_gets_no_file_but_a_partly_numeric_one_does() {
        let t = table(
            "per_cpu",
            "# hostname;interval;timestamp;CPU;%user;%idle;%steal\n\
             h;1;2026-11-25 10:00:00 UTC;0;1.0;text;7.0\n\
             h;1;2026-11-25 10:00:00 UTC;1;2.0;more;oops\n",
        );
        let files = pivot(&t);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["per_cpu__user.csv", "per_cpu__steal.csv"]);
        assert!(file(&files, "per_cpu__steal.csv").contains("2026-11-25 10:00:00 UTC;7.0;0.0"));
    }

    #[test]
    fn file_suffix_matches_legacy_names() {
        assert_eq!(file_suffix("%util"), "_util");
        assert_eq!(file_suffix("rkB/s"), "rkB_s");
        assert_eq!(file_suffix("CPU*"), "CPU_");
        assert_eq!(file_suffix("areq-sz"), "areq-sz");
    }

    #[test]
    fn activities_without_dimension_do_not_pivot() {
        let queue = table(
            "queue",
            "# hostname;interval;timestamp;runq-sz\nh;1;2026-11-25 10:00:00 UTC;1\n",
        );
        assert!(pivot(&queue).is_empty());
    }
}
