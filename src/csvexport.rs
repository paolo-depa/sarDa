//! `--export-csv`: the per-Activity CSV files the original Python tool wrote,
//! including one pivoted file per metric for Activities with a Dimension.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::catalog::{Activity, HOST_COLUMN, INTERVAL_COLUMN, TIMESTAMP_COLUMN};
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

/// Pivoted CSV files for `table`: `(file name, content)` per metric.
pub fn pivot(activity: &Activity, table: &Table) -> Vec<(String, String)> {
    let Some(dim) = &activity.dimension else {
        return Vec::new();
    };
    let (Some(ts_idx), Some(dim_idx)) = (table.column(TIMESTAMP_COLUMN), table.column(dim.column))
    else {
        return Vec::new();
    };

    let mut dims: Vec<String> = table
        .rows
        .iter()
        .map(|r| r[dim_idx].clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sort_dimension_values(&mut dims);
    let dim_pos: BTreeMap<&str, usize> = dims
        .iter()
        .enumerate()
        .map(|(i, d)| (d.as_str(), i))
        .collect();

    let mut stamps: BTreeMap<i64, String> = BTreeMap::new();
    for r in &table.rows {
        if let Some(t) = timeutil::parse_sadf(&r[ts_idx]) {
            stamps.entry(t).or_insert_with(|| r[ts_idx].clone());
        }
    }

    let mut files = Vec::new();
    for (c, name) in table.header.iter().enumerate() {
        if c == ts_idx || c == dim_idx || name == HOST_COLUMN || name == INTERVAL_COLUMN {
            continue;
        }
        let mut grid: BTreeMap<i64, Vec<f64>> =
            stamps.keys().map(|t| (*t, vec![0.0; dims.len()])).collect();
        let mut numeric = true;
        for r in &table.rows {
            let Some(t) = timeutil::parse_sadf(&r[ts_idx]) else {
                continue;
            };
            match r[c].trim().parse::<f64>() {
                Ok(v) => grid.get_mut(&t).unwrap()[dim_pos[r[dim_idx].as_str()]] = v,
                Err(_) => {
                    numeric = false;
                    break;
                }
            }
        }
        if !numeric {
            continue;
        }
        let sep = SEPARATOR.to_string();
        let mut out = format!("{TIMESTAMP_COLUMN}{sep}{}\n", dims.join(&sep));
        for (t, vals) in &grid {
            out.push_str(&stamps[t]);
            for v in vals {
                out.push(SEPARATOR);
                out.push_str(&format_value(*v));
            }
            out.push('\n');
        }
        files.push((format!("{}_{}.csv", activity.label, file_suffix(name)), out));
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
    for (activity, table) in &ds.tables {
        write(&format!("{}.csv", activity.label), &table.to_csv())?;
        count += 1;
        if activity.csv_pivot {
            for (name, content) in pivot(activity, table) {
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

    fn table() -> Table {
        Table::parse(
            "# hostname;interval;timestamp;CPU;%user;%idle\n\
             h;1;2026-11-25 10:00:00 UTC;-1;10.00;90.00\n\
             h;1;2026-11-25 10:00:00 UTC;10;5.00;95.00\n\
             h;1;2026-11-25 10:00:00 UTC;2;20.00;80.00\n\
             h;1;2026-11-25 10:00:01 UTC;2;21.50;78.50\n",
        )
        .unwrap()
        .0
    }

    #[test]
    fn pivot_orders_numeric_dimensions_and_zero_fills() {
        let files = pivot(activity("per_cpu").unwrap(), &table());
        let user = &files
            .iter()
            .find(|(n, _)| n == "per_cpu__user.csv")
            .unwrap()
            .1;
        let lines: Vec<&str> = user.lines().collect();
        assert_eq!(lines[0], "timestamp;-1;2;10");
        assert_eq!(lines[1], "2026-11-25 10:00:00 UTC;10.0;20.0;5.0");
        assert_eq!(lines[2], "2026-11-25 10:00:01 UTC;0.0;21.5;0.0");
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
        assert!(pivot(activity("queue").unwrap(), &table()).is_empty());
    }
}
