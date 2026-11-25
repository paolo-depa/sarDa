//! Turning raw `sadf` output into one cleaned [`Table`] per Activity.

use std::collections::BTreeSet;

use crate::catalog::{Activity, ACTIVITIES};
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
    pub tables: Vec<(&'static Activity, Table)>,
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
            let Some((table, bad)) = Table::parse(&raw.stdout) else {
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
                tables.push((activity, table));
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
