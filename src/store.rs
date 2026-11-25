//! In-memory time series built from a [`Dataset`], queried by the Sarda API.

use std::collections::{BTreeMap, HashMap};

use crate::dataset::Dataset;

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

        for table in &ds.tables {
            let (activity, layout) = (table.activity, table.layout());
            let mut acc: HashMap<(usize, String, String), Vec<(i64, f64)>> = HashMap::new();
            for rec in &table.records {
                let ts_ms = rec.ts * 1000;
                min_ts = min_ts.min(ts_ms);
                max_ts = max_ts.max(ts_ms);
                if let Some(secs) = table.interval(rec) {
                    intervals.push((secs * 1000.0) as i64);
                }
                let host = table.host(rec).to_string();
                let dim = match (&activity.dimension, table.dimension(rec)) {
                    (Some(d), Some(raw)) => d.label_value(raw),
                    _ => String::new(),
                };
                for &c in &layout.metrics {
                    if let Some(v) = rec.number(c) {
                        acc.entry((c, host.clone(), dim.clone()))
                            .or_default()
                            .push((ts_ms, v));
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
                if layout.host.is_some() {
                    labels.insert("host".into(), host);
                }
                if let (Some(d), Some(_)) = (&activity.dimension, layout.dimension) {
                    labels.insert(d.label.into(), dim);
                }
                series.push(Series {
                    name: table.metric_name(c),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::activity;
    use crate::table::Table;

    fn store(label: &str, text: &str) -> Store {
        let table = Table::parse(text, activity(label).unwrap()).unwrap().0;
        Store::from_dataset(&Dataset {
            tables: vec![table],
            warnings: vec![],
        })
    }

    fn samples(store: &Store, metric: &str, label: (&str, &str)) -> Vec<(i64, f64)> {
        store
            .by_name(metric)
            .find(|s| s.labels.get(label.0).map(String::as_str) == Some(label.1))
            .unwrap_or_else(|| panic!("no series {metric} {label:?}"))
            .samples
            .clone()
    }

    #[test]
    fn non_finite_values_leave_a_gap_in_the_series() {
        let s = store(
            "disk",
            "# hostname;interval;timestamp;DEV;await\n\
             h;1;2026-11-25 10:00:00 UTC;sda;-nan\n\
             h;1;2026-11-25 10:00:01 UTC;sda;5.0\n\
             h;1;2026-11-25 10:00:02 UTC;sda;inf\n",
        );
        let got = samples(&s, "sarda_disk_await", ("dev", "sda"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, 5.0);
    }

    #[test]
    fn labels_carry_host_and_entity_and_all_cpus_are_all() {
        let s = store(
            "per_cpu",
            "# hostname;interval;timestamp;CPU;%user\n\
             h;60;2026-11-25 10:00:00 UTC;-1;1.0\n\
             h;60;2026-11-25 10:00:00 UTC; 3 ;2.0\n",
        );
        let all = samples(&s, "sarda_per_cpu_pct_user", ("cpu", "all"));
        assert_eq!(all[0].1, 1.0);
        let third = s
            .by_name("sarda_per_cpu_pct_user")
            .find(|x| x.labels["cpu"] == "3")
            .unwrap();
        assert_eq!(third.labels["host"], "h");
        assert_eq!(s.interval_ms, 60_000);
    }

    #[test]
    fn the_last_record_wins_for_a_repeated_timestamp() {
        let s = store(
            "io",
            "# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:00 UTC;1\n\
             h;1;2026-11-25 10:00:00 UTC;9\n",
        );
        let series = s.by_name("sarda_io_tps").next().unwrap();
        assert_eq!(series.samples.len(), 1);
        assert_eq!(series.samples[0].1, 9.0);
    }
}
