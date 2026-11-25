//! Cleaning, merging and interpreting the CSV text `sadf -d` produces for one
//! Activity. This is the only module that turns `sadf`'s text cells into
//! timestamps and numbers; everything downstream asks a [`Table`].
//!
//! The raw cells are kept next to the parsed values because the plain
//! `<label>.csv` export must reproduce `sadf`'s text exactly (`0.00`, not `0`).

use crate::catalog::{self, Activity, HOST_COLUMN, INTERVAL_COLUMN, TIMESTAMP_COLUMN};
use crate::timeutil;

pub const SEPARATOR: char = ';';

/// One parsed line of `sadf -d` output.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Unix seconds, parsed once.
    pub ts: i64,
    cells: Vec<String>,
}

impl Record {
    /// A cell as a finite number. Text, blanks, `-nan` and `inf` are missing.
    pub fn number(&self, col: usize) -> Option<f64> {
        self.cells[col]
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }

    fn text(&self, col: usize) -> &str {
        self.cells[col].trim()
    }
}

/// Which columns of a table play which part, per the Activity's catalog entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub host: Option<usize>,
    pub interval: Option<usize>,
    /// The column that splits the Activity into entities, when it has one.
    pub dimension: Option<usize>,
    /// Every remaining column, in header order.
    pub metrics: Vec<usize>,
}

/// One Activity's records, possibly merged from several SA files.
#[derive(Debug, Clone)]
pub struct Table {
    pub activity: &'static Activity,
    header: Vec<String>,
    pub records: Vec<Record>,
    layout: Layout,
}

#[derive(Debug, Default)]
pub struct MergeReport {
    pub warnings: Vec<String>,
}

impl Table {
    /// Parses sadf output for `activity`: the first non-empty line is the
    /// header, `RESTART` markers and comment lines are dropped, and lines whose
    /// cell count does not match the header or whose timestamp cannot be read
    /// are discarded (counted in the second element). Returns `None` when
    /// there is no usable header with a timestamp column.
    pub fn parse(text: &str, activity: &'static Activity) -> Option<(Table, usize)> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let header: Vec<String> = lines
            .next()?
            .split(SEPARATOR)
            .map(|h| h.trim().to_string())
            .collect();
        let ts_col = header.iter().position(|h| h == TIMESTAMP_COLUMN)?;
        let mut records = Vec::new();
        let mut bad = 0;
        for line in lines {
            if line.contains("RESTART") || line.starts_with('#') {
                continue;
            }
            let cells: Vec<String> = line.split(SEPARATOR).map(str::to_string).collect();
            let ts = (cells.len() == header.len())
                .then(|| timeutil::parse_sadf(&cells[ts_col]))
                .flatten();
            match ts {
                Some(ts) => records.push(Record { ts, cells }),
                None => bad += 1,
            }
        }
        let layout = Layout::of(activity, &header);
        Some((
            Table {
                activity,
                header,
                records,
                layout,
            },
            bad,
        ))
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn column_name(&self, col: usize) -> &str {
        &self.header[col]
    }

    /// The Prometheus metric name of column `col`.
    pub fn metric_name(&self, col: usize) -> String {
        catalog::metric_name(self.activity.label, &self.header[col])
    }

    /// The host that wrote the record, or `""` when the table has no host column.
    pub fn host<'a>(&self, r: &'a Record) -> &'a str {
        self.layout.host.map_or("", |c| r.text(c))
    }

    /// The raw entity of the record (`-1` for all CPUs), when the Activity has
    /// a Dimension. [`catalog::Dimension::label_value`] gives the label.
    pub fn dimension<'a>(&self, r: &'a Record) -> Option<&'a str> {
        self.layout.dimension.map(|c| r.text(c))
    }

    /// The sampling interval in seconds, when the record states a positive one.
    pub fn interval(&self, r: &Record) -> Option<f64> {
        self.layout
            .interval
            .and_then(|c| r.number(c))
            .filter(|v| *v > 0.0)
    }

    /// Appends `other`'s records when the headers match; otherwise reports
    /// why it was skipped.
    pub fn merge(&mut self, other: Table, source: &str, report: &mut MergeReport) {
        if other.header != self.header {
            report.warnings.push(format!(
                "{source}: columns differ from the first file ({} vs {}); skipped",
                other.header.len(),
                self.header.len()
            ));
            return;
        }
        self.records.extend(other.records);
    }

    /// Sorts records chronologically (stable) and keeps only those inside the
    /// window.
    pub fn restrict(&mut self, start: Option<i64>, end: Option<i64>) {
        self.records
            .retain(|r| start.is_none_or(|s| r.ts >= s) && end.is_none_or(|e| r.ts <= e));
        self.records.sort_by_key(|r| r.ts);
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The table as `sadf` printed it, merged and in time order.
    pub fn to_csv(&self) -> String {
        let sep = SEPARATOR.to_string();
        let mut out = self.header.join(&sep);
        out.push('\n');
        for r in &self.records {
            out.push_str(&r.cells.join(&sep));
            out.push('\n');
        }
        out
    }
}

impl Layout {
    fn of(activity: &Activity, header: &[String]) -> Layout {
        let find = |name: &str| header.iter().position(|h| h == name);
        Layout {
            host: find(HOST_COLUMN),
            interval: find(INTERVAL_COLUMN),
            dimension: activity.dimension.as_ref().and_then(|d| find(d.column)),
            metrics: header
                .iter()
                .enumerate()
                .filter(|(_, h)| !activity.is_meta_column(h))
                .map(|(i, _)| i)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::activity;

    const SAMPLE: &str = "# hostname;interval;timestamp;tps\n\
        h;1;2026-11-25 10:00:00 UTC;1.00\n\
        # hostname;interval;timestamp;tps\n\
        LINUX-RESTART\n\
        h;1;2026-11-25 10:00:01 UTC;2.00\n\
        h;1;broken\n\
        h;1;not a time;3.00\n";

    fn io() -> &'static Activity {
        activity("io").unwrap()
    }

    fn table(text: &str, label: &str) -> Table {
        Table::parse(text, activity(label).unwrap()).unwrap().0
    }

    #[test]
    fn parse_drops_noise_and_counts_unusable_lines() {
        let (t, bad) = Table::parse(SAMPLE, io()).unwrap();
        assert_eq!(t.column_name(2), "timestamp");
        assert_eq!(t.records.len(), 2);
        assert_eq!(bad, 2, "wrong cell count and unparseable timestamp");
        assert_eq!(t.records[1].ts - t.records[0].ts, 1);
    }

    #[test]
    fn parse_requires_timestamp_header() {
        assert!(Table::parse("a;b\n1;2\n", io()).is_none());
        assert!(Table::parse("", io()).is_none());
    }

    #[test]
    fn layout_names_the_roles_of_the_columns() {
        let cpu = table(
            "# hostname;interval;timestamp;CPU;%user;%idle\nh;1;2026-11-25 10:00:00 UTC;-1;1;2\n",
            "per_cpu",
        );
        assert_eq!(
            cpu.layout(),
            &Layout {
                host: Some(0),
                interval: Some(1),
                dimension: Some(3),
                metrics: vec![4, 5],
            }
        );
        assert_eq!(cpu.metric_name(4), "sarda_per_cpu_pct_user");

        let io = table(SAMPLE, "io");
        assert_eq!(io.layout().dimension, None);
        assert_eq!(io.layout().metrics, vec![3]);
    }

    #[test]
    fn a_dimension_missing_from_the_header_is_not_a_column() {
        let t = table(
            "# hostname;interval;timestamp;%user\nh;1;2026-11-25 10:00:00 UTC;1\n",
            "per_cpu",
        );
        assert_eq!(t.layout().dimension, None);
        assert_eq!(t.layout().metrics, vec![3]);
    }

    #[test]
    fn numbers_are_finite_or_missing() {
        let t = table(
            "# hostname;interval;timestamp;a;b;c;d;e;f;g\n\
             h;1;2026-11-25 10:00:00 UTC;1.50; 2 ;-nan;inf;-inf;text;\n",
            "io",
        );
        let r = &t.records[0];
        let got: Vec<Option<f64>> = (3..=9).map(|c| r.number(c)).collect();
        assert_eq!(got, [Some(1.5), Some(2.0), None, None, None, None, None]);
    }

    #[test]
    fn accessors_read_host_dimension_and_interval() {
        let t = table(
            "# hostname;interval;timestamp;CPU;%user\n\
             h1;60;2026-11-25 10:00:00 UTC; -1 ;1\n\
             h2;-1;2026-11-25 10:00:00 UTC;3;2\n",
            "per_cpu",
        );
        let (a, b) = (&t.records[0], &t.records[1]);
        assert_eq!((t.host(a), t.host(b)), ("h1", "h2"));
        assert_eq!((t.dimension(a), t.dimension(b)), (Some("-1"), Some("3")));
        assert_eq!(t.interval(a), Some(60.0));
        assert_eq!(t.interval(b), None, "sadf prints -1 when not applicable");

        let plain = table(SAMPLE, "io");
        assert_eq!(plain.dimension(&plain.records[0]), None);
    }

    #[test]
    fn merge_appends_matching_and_skips_mismatching() {
        let mut a = table(SAMPLE, "io");
        let b = table(SAMPLE, "io");
        let c = table(
            "# hostname;interval;timestamp;other\nh;1;2026-11-25 10:00:09 UTC;1\n",
            "io",
        );
        let mut report = MergeReport::default();
        a.merge(b, "b", &mut report);
        a.merge(c, "c", &mut report);
        assert_eq!(a.records.len(), 4);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn restrict_sorts_and_filters_inclusively() {
        let mut t = table(
            "# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:02 UTC;3\n\
             h;1;2026-11-25 10:00:00 UTC;1\n\
             h;1;2026-11-25 10:00:01 UTC;2\n",
            "io",
        );
        let from = timeutil::parse_sadf("2026-11-25 10:00:01 UTC");
        t.restrict(from, None);
        let cells: Vec<f64> = t.records.iter().map(|r| r.number(3).unwrap()).collect();
        assert_eq!(cells, [2.0, 3.0]);
    }

    #[test]
    fn to_csv_reproduces_the_cells_verbatim() {
        let t = table(
            "# hostname;interval;timestamp;tps\nh;1;2026-11-25 10:00:00 UTC;0.00\n",
            "io",
        );
        assert_eq!(
            t.to_csv(),
            "# hostname;interval;timestamp;tps\nh;1;2026-11-25 10:00:00 UTC;0.00\n"
        );
    }
}
