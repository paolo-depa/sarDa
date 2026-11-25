//! Cleaning and merging of the CSV text `sadf -d` produces for one Activity.

use crate::catalog::TIMESTAMP_COLUMN;
use crate::timeutil;

pub const SEPARATOR: char = ';';

/// One Activity's rows, possibly merged from several SA files.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct MergeReport {
    pub warnings: Vec<String>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }

    /// Parses sadf output: the first non-empty line is the header, `RESTART`
    /// markers and comment lines are dropped, and rows whose cell count does
    /// not match the header are discarded (counted in the second element).
    /// Returns `None` when there is no usable header with a timestamp column.
    pub fn parse(text: &str) -> Option<(Table, usize)> {
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        let header: Vec<String> = lines
            .next()?
            .split(SEPARATOR)
            .map(|h| h.trim().to_string())
            .collect();
        if !header.iter().any(|h| h == TIMESTAMP_COLUMN) {
            return None;
        }
        let mut rows = Vec::new();
        let mut bad = 0;
        for line in lines {
            if line.contains("RESTART") || line.starts_with('#') {
                continue;
            }
            let cells: Vec<String> = line.split(SEPARATOR).map(str::to_string).collect();
            if cells.len() == header.len() {
                rows.push(cells);
            } else {
                bad += 1;
            }
        }
        Some((Table { header, rows }, bad))
    }

    /// Appends `other`'s rows when the headers match; otherwise reports why
    /// it was skipped.
    pub fn merge(&mut self, other: Table, source: &str, report: &mut MergeReport) {
        if other.header != self.header {
            report.warnings.push(format!(
                "{source}: columns differ from the first file ({} vs {}); skipped",
                other.header.len(),
                self.header.len()
            ));
            return;
        }
        self.rows.extend(other.rows);
    }

    /// Sorts rows chronologically (stable) and keeps only those inside the
    /// window. Rows with an unparseable timestamp are dropped.
    pub fn restrict(&mut self, start: Option<i64>, end: Option<i64>) {
        let Some(ts_idx) = self.column(TIMESTAMP_COLUMN) else {
            return;
        };
        let mut keyed: Vec<(i64, Vec<String>)> = std::mem::take(&mut self.rows)
            .into_iter()
            .filter_map(|r| timeutil::parse_sadf(&r[ts_idx]).map(|t| (t, r)))
            .filter(|(t, _)| start.is_none_or(|s| *t >= s) && end.is_none_or(|e| *t <= e))
            .collect();
        keyed.sort_by_key(|(t, _)| *t);
        self.rows = keyed.into_iter().map(|(_, r)| r).collect();
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn to_csv(&self) -> String {
        let sep = SEPARATOR.to_string();
        let mut out = self.header.join(&sep);
        out.push('\n');
        for row in &self.rows {
            out.push_str(&row.join(&sep));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# hostname;interval;timestamp;tps\n\
        h;1;2026-11-25 10:00:00 UTC;1.00\n\
        # hostname;interval;timestamp;tps\n\
        LINUX-RESTART\n\
        h;1;2026-11-25 10:00:01 UTC;2.00\n\
        h;1;broken\n";

    #[test]
    fn parse_drops_noise_and_counts_bad_rows() {
        let (t, bad) = Table::parse(SAMPLE).unwrap();
        assert_eq!(t.header[2], "timestamp");
        assert_eq!(t.rows.len(), 2);
        assert_eq!(bad, 1);
    }

    #[test]
    fn parse_requires_timestamp_header() {
        assert!(Table::parse("a;b\n1;2\n").is_none());
        assert!(Table::parse("").is_none());
    }

    #[test]
    fn merge_appends_matching_and_skips_mismatching() {
        let (mut a, _) = Table::parse(SAMPLE).unwrap();
        let (b, _) = Table::parse(SAMPLE).unwrap();
        let (c, _) =
            Table::parse("# hostname;interval;timestamp;other\nh;1;2026-11-25 10:00:09 UTC;1\n")
                .unwrap();
        let mut report = MergeReport::default();
        a.merge(b, "b", &mut report);
        a.merge(c, "c", &mut report);
        assert_eq!(a.rows.len(), 4);
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn restrict_sorts_and_filters_inclusively() {
        let (mut t, _) = Table::parse(
            "# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:02 UTC;3\n\
             h;1;2026-11-25 10:00:00 UTC;1\n\
             h;1;2026-11-25 10:00:01 UTC;2\n",
        )
        .unwrap();
        let from = timeutil::parse_sadf("2026-11-25 10:00:01 UTC");
        t.restrict(from, None);
        let cells: Vec<&str> = t.rows.iter().map(|r| r[3].as_str()).collect();
        assert_eq!(cells, ["2", "3"]);
    }
}
