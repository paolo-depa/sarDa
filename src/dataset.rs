//! Turning raw `sadf` output into one cleaned [`Table`] per Activity.

use std::collections::BTreeSet;

use crate::catalog::ACTIVITIES;
use crate::error::{Error, Kind, Result};
use crate::sadf::SadfOutput;
use crate::table::{MergeReport, Table};

/// stderr fragments `sadf` prints for Activities a file simply lacks.
const BENIGN_STDERR: [&str; 3] = [
    "end of file",
    "no data",
    "requested activities not available",
];

#[derive(Debug)]
pub struct Dataset {
    /// Non-empty tables in catalog order.
    pub tables: Vec<Table>,
    pub warnings: Vec<String>,
}

pub fn assemble(output: &SadfOutput, start: Option<i64>, end: Option<i64>) -> Result<Dataset> {
    let mut warnings: Vec<String> = Vec::new();
    let mut seen_stderr: BTreeSet<(usize, String)> = BTreeSet::new();
    let mut tables = Vec::new();

    for activity in ACTIVITIES {
        let mut merged: Option<Table> = None;
        let mut report = MergeReport::default();

        for (idx, file) in output.files.iter().enumerate() {
            let name = output.name(idx);
            let Some(raw) = file.iter().find(|r| r.label == activity.label) else {
                continue;
            };
            for line in &raw.stderr {
                let lower = line.to_lowercase();
                if !BENIGN_STDERR.iter().any(|b| lower.contains(b))
                    && seen_stderr.insert((idx, line.clone()))
                {
                    warnings.push(format!("{name}: sadf: {line}"));
                }
            }
            let Some((table, bad)) = Table::parse(&raw.stdout, activity) else {
                continue;
            };
            if bad > 0 {
                warnings.push(format!(
                    "{name}: {}: {bad} malformed row(s) ignored",
                    activity.label
                ));
            }
            match merged.as_mut() {
                None => merged = Some(table),
                Some(m) => m.merge(table, name, &mut report),
            }
        }
        warnings.extend(
            report
                .warnings
                .into_iter()
                .map(|w| format!("{}: {w}", activity.label)),
        );

        if let Some(mut table) = merged {
            table.restrict(start, end);
            if !table.is_empty() {
                tables.push(table);
            }
        }
    }

    if tables.is_empty() {
        let mut msg = String::from("no usable data found in the SA files");
        if start.is_some() || end.is_some() {
            msg.push_str(
                " (check that --start/--end overlap the data; times without an offset are UTC)",
            );
        }
        for w in warnings.iter().take(5) {
            msg.push_str(&format!("\n  - {w}"));
        }
        return Err(Error::new(Kind::NoData, msg));
    }
    Ok(Dataset { tables, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sadf::{parse_stream, Input};

    fn output(stream: &str) -> SadfOutput {
        parse_stream(stream.as_bytes())
            .unwrap()
            .with_inputs(&[Input::new("sa01")])
    }

    #[test]
    fn records_with_a_bad_timestamp_are_reported_not_silently_dropped() {
        let out = output(
            "@@FILE 0\n@@BEGIN io\n# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:00 UTC;1\nh;1;garbage;2\n@@END\n",
        );
        let ds = assemble(&out, None, None).unwrap();
        assert_eq!(ds.tables[0].records.len(), 1);
        assert_eq!(ds.warnings, ["sa01: io: 1 malformed row(s) ignored"]);
    }

    #[test]
    fn benign_stderr_is_not_a_warning_but_other_stderr_is() {
        let out = output(
            "@@FILE 0\n@@BEGIN io\n# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:00 UTC;1\n@@ERR Requested activities not available\n\
             @@ERR something odd\n@@ERR something odd\n@@END\n",
        );
        let ds = assemble(&out, None, None).unwrap();
        assert_eq!(ds.warnings, ["sa01: sadf: something odd"]);
    }

    #[test]
    fn nothing_usable_is_a_no_data_error_that_mentions_the_window() {
        let out = output(
            "@@FILE 0\n@@BEGIN io\n# hostname;interval;timestamp;tps\n\
             h;1;2026-11-25 10:00:00 UTC;1\n@@END\n",
        );
        let err = assemble(&out, Some(i64::MAX), None).unwrap_err();
        assert_eq!(err.kind, Kind::NoData);
        assert!(err.message.contains("--start/--end"));
    }
}
