//! In-memory time series built from a [`Dataset`], queried by the Sarda API.

use std::collections::{BTreeMap, HashMap};

use crate::catalog::{metric_name, HOST_COLUMN, INTERVAL_COLUMN, TIMESTAMP_COLUMN};
use crate::dataset::Dataset;
use crate::timeutil;

pub type Labels = BTreeMap<String, String>;

#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub labels: Labels,
    /// `(unix milliseconds, value)`, sorted by time, one sample per timestamp.
    pub samples: Vec<(i64, f64)>,
}

#[derive(Debug, Default)]
pub struct Store {
    pub series: Vec<Series>,
    by_name: HashMap<String, Vec<usize>>,
    /// Typical gap between samples, used to size the query lookback window.
    pub interval_ms: i64,
    pub min_ts_ms: i64,
    pub max_ts_ms: i64,
}

impl Store {
    pub fn from_dataset(ds: &Dataset) -> Store {
        let mut series = Vec::new();
        let mut intervals: Vec<i64> = Vec::new();
        let (mut min_ts, mut max_ts) = (i64::MAX, i64::MIN);

        for (activity, table) in &ds.tables {
            let Some(ts_idx) = table.column(TIMESTAMP_COLUMN) else {
                continue;
            };
            let host_idx = table.column(HOST_COLUMN);
            let int_idx = table.column(INTERVAL_COLUMN);
            let dim_idx = activity
                .dimension
                .as_ref()
                .and_then(|d| table.column(d.column));
            let value_cols: Vec<usize> = table
                .header
                .iter()
                .enumerate()
                .filter(|(_, h)| !activity.is_meta_column(h))
                .map(|(i, _)| i)
                .collect();

            let mut acc: HashMap<(usize, String, String), Vec<(i64, f64)>> = HashMap::new();
            for row in &table.rows {
                let Some(ts) = timeutil::parse_sadf(&row[ts_idx]) else {
                    continue;
                };
                let ts_ms = ts * 1000;
                min_ts = min_ts.min(ts_ms);
                max_ts = max_ts.max(ts_ms);
                if let Some(i) = int_idx {
                    if let Ok(v) = row[i].trim().parse::<f64>() {
                        if v > 0.0 {
                            intervals.push((v * 1000.0) as i64);
                        }
                    }
                }
                let host = host_idx.map_or(String::new(), |i| row[i].clone());
                let dim = dim_idx.map_or(String::new(), |i| {
                    dimension_value(activity.dimension.as_ref().unwrap().column, &row[i])
                });
                for &c in &value_cols {
                    if let Ok(v) = row[c].trim().parse::<f64>() {
                        if v.is_finite() {
                            acc.entry((c, host.clone(), dim.clone()))
                                .or_default()
                                .push((ts_ms, v));
                        }
                    }
                }
            }

            let mut keys: Vec<_> = acc.keys().cloned().collect();
            keys.sort();
            for key in keys {
                let mut samples = acc.remove(&key).unwrap();
                samples.sort_by_key(|(t, _)| *t);
                samples.dedup_by(|later, earlier| {
                    if later.0 == earlier.0 {
                        earlier.1 = later.1;
                        true
                    } else {
                        false
                    }
                });
                let (c, host, dim) = key;
                let mut labels = Labels::new();
                if host_idx.is_some() {
                    labels.insert("host".into(), host);
                }
                if let Some(d) = &activity.dimension {
                    if dim_idx.is_some() {
                        labels.insert(d.label.into(), dim);
                    }
                }
                series.push(Series {
                    name: metric_name(activity.label, &table.header[c]),
                    labels,
                    samples,
                });
            }
        }

        intervals.sort_unstable();
        let mut store = Store::from_series(series);
        store.interval_ms = intervals
            .get(intervals.len() / 2)
            .copied()
            .unwrap_or(10_000);
        if min_ts <= max_ts {
            store.min_ts_ms = min_ts;
            store.max_ts_ms = max_ts;
        }
        store
    }

    /// Builds a store (with a default 10 s interval) from ready-made series.
    pub fn from_series(series: Vec<Series>) -> Store {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let (mut min_ts, mut max_ts) = (i64::MAX, i64::MIN);
        for (i, s) in series.iter().enumerate() {
            by_name.entry(s.name.clone()).or_default().push(i);
            if let (Some(first), Some(last)) = (s.samples.first(), s.samples.last()) {
                min_ts = min_ts.min(first.0);
                max_ts = max_ts.max(last.0);
            }
        }
        if min_ts > max_ts {
            min_ts = 0;
            max_ts = 0;
        }
        Store {
            series,
            by_name,
            interval_ms: 10_000,
            min_ts_ms: min_ts,
            max_ts_ms: max_ts,
        }
    }

    pub fn by_name(&self, name: &str) -> impl Iterator<Item = &Series> {
        self.by_name
            .get(name)
            .into_iter()
            .flat_map(|idx| idx.iter().map(|&i| &self.series[i]))
    }

    pub fn metric_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.by_name.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Sorted distinct values of `host` across all series.
    pub fn hosts(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .series
            .iter()
            .filter_map(|s| s.labels.get("host").cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

fn dimension_value(column: &str, raw: &str) -> String {
    if column == "CPU" && raw.trim() == "-1" {
        "all".to_string()
    } else {
        raw.trim().to_string()
    }
}
