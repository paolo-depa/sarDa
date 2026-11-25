//! Grafana dashboard generation from the Metric catalog and the data actually
//! found. Panels follow what the SA files *collected*, not what the values
//! happen to be: a recorded Activity gets its panels even when every sample
//! is zero (an idle device, a host that never swapped), while an Activity the
//! file does not contain gets no row and no panel. Per-entity panels use label
//! variables so any host's devices work.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::catalog::{metric_name, unit_for, Activity, HOST_COLUMN};
use crate::dataset::Dataset;
use crate::store::Store;
use crate::timeutil;

pub const DASHBOARD_UID: &str = "sarda";
pub const DATASOURCE_UID: &str = "sarda";

/// Rows shown in this order; the first few start expanded, the rest are
/// collapsed so Grafana does not fire hundreds of queries on load.
const GROUP_ORDER: [&str; 5] = ["CPU", "Memory", "Queue and load", "Disks", "IO"];
const EXPANDED_GROUPS: usize = 3;

struct Ids {
    next: u32,
}

impl Ids {
    fn take(&mut self) -> u32 {
        self.next += 1;
        self.next
    }
}

fn datasource() -> Value {
    json!({"type": "prometheus", "uid": DATASOURCE_UID})
}

struct Ctx {
    multi_host: bool,
    interval: String,
}

fn selector(metric: &str, dim_label: Option<&str>) -> String {
    match dim_label {
        Some(l) => format!("{metric}{{host=~\"$host\", {l}=~\"${l}\"}}"),
        None => format!("{metric}{{host=~\"$host\"}}"),
    }
}

fn target(ref_id: usize, expr: String, legend: String, ctx: &Ctx) -> Value {
    json!({
        "refId": ref_id_letter(ref_id),
        "datasource": datasource(),
        "expr": expr,
        "legendFormat": legend,
        "range": true,
        "instant": false,
        "interval": ctx.interval,
    })
}

fn ref_id_letter(i: usize) -> String {
    let mut n = i;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            return s;
        }
        n = n / 26 - 1;
    }
}

fn timeseries(
    id: u32,
    title: &str,
    unit: &str,
    targets: Vec<Value>,
    legend_table: bool,
    ctx: &Ctx,
) -> Value {
    json!({
        "id": id,
        "type": "timeseries",
        "title": title,
        "datasource": datasource(),
        "interval": ctx.interval,
        "fieldConfig": {
            "defaults": {
                "unit": unit,
                "custom": {
                    "drawStyle": "line",
                    "lineWidth": 1,
                    "fillOpacity": 8,
                    "showPoints": "never",
                    "spanNulls": true
                }
            },
            "overrides": []
        },
        "options": {
            "legend": {
                "showLegend": true,
                "displayMode": if legend_table { "table" } else { "list" },
                "placement": "bottom",
                "calcs": if legend_table { json!(["mean", "max", "lastNotNull"]) } else { json!([]) }
            },
            "tooltip": {"mode": "multi", "sort": "desc"}
        },
        "targets": targets,
    })
}

/// Metric columns of `activity` that produced at least one series, in header
/// order (text columns such as `DEVICE` are skipped).
fn value_columns<'a>(activity: &Activity, header: &'a [String], store: &Store) -> Vec<&'a str> {
    header
        .iter()
        .map(String::as_str)
        .filter(|h| !activity.is_meta_column(h) && *h != HOST_COLUMN)
        .filter(|h| {
            store
                .by_name(&metric_name(activity.label, h))
                .next()
                .is_some()
        })
        .collect()
}

/// Panels (without `gridPos`) for one Activity.
fn activity_panels(
    activity: &Activity,
    header: &[String],
    store: &Store,
    ids: &mut Ids,
    ctx: &Ctx,
) -> Vec<Value> {
    let columns = value_columns(activity, header, store);
    let mut panels = Vec::new();

    if let Some(dim) = &activity.dimension {
        for col in &columns {
            let legend = if ctx.multi_host {
                format!("{{{{host}}}} {{{{{}}}}}", dim.label)
            } else {
                format!("{{{{{}}}}}", dim.label)
            };
            let expr = selector(&metric_name(activity.label, col), Some(dim.label));
            panels.push(timeseries(
                ids.take(),
                &format!("{}: {col}", activity.description),
                unit_for(col),
                vec![target(0, expr, legend, ctx)],
                false,
                ctx,
            ));
        }
        return panels;
    }

    let mut covered: BTreeSet<&str> = BTreeSet::new();
    let mut groups: Vec<(String, Vec<&str>)> = Vec::new();
    for spec in activity.panels {
        let present: Vec<&str> = spec
            .columns
            .iter()
            .copied()
            .filter(|c| columns.contains(c))
            .collect();
        covered.extend(present.iter().copied());
        if !present.is_empty() {
            groups.push((spec.title.to_string(), present));
        }
    }
    let rest: Vec<&str> = columns
        .iter()
        .copied()
        .filter(|c| !covered.contains(c))
        .collect();
    if !rest.is_empty() {
        let title = if groups.is_empty() {
            activity.description.to_string()
        } else {
            "Other".to_string()
        };
        groups.push((title, rest));
    }

    for (title, cols) in groups {
        let targets: Vec<Value> = cols
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let legend = if ctx.multi_host {
                    format!("{col} {{{{host}}}}")
                } else {
                    (*col).to_string()
                };
                target(
                    i,
                    selector(&metric_name(activity.label, col), None),
                    legend,
                    ctx,
                )
            })
            .collect();
        let panel_title = if activity.panels.is_empty() || title == "Other" {
            title
        } else {
            format!("{}: {title}", activity.description)
        };
        panels.push(timeseries(
            ids.take(),
            &panel_title,
            unit_for(cols[0]),
            targets,
            true,
            ctx,
        ));
    }
    panels
}

/// Variables that are shown in the dashboard header; the rest (power
/// sensors, USB buses, ...) stay available in the URL but are hidden.
const VISIBLE_VARIABLES: [&str; 6] = ["host", "cpu", "dev", "iface", "filesystem", "intr"];

fn variable(name: &str, query: &str) -> Value {
    json!({
        "name": name,
        "label": name,
        "type": "query",
        "datasource": datasource(),
        "definition": query,
        "query": {
            "query": query,
            "refId": "PrometheusVariableQueryEditor-VariableQuery"
        },
        "refresh": 2,
        "multi": true,
        "includeAll": true,
        "allValue": ".*",
        "current": {"selected": true, "text": ["All"], "value": ["$__all"]},
        "sort": 1,
        "hide": if VISIBLE_VARIABLES.contains(&name) { 0 } else { 2 }
    })
}

fn interval_text(interval_ms: i64) -> String {
    let secs = (interval_ms / 1000).max(1);
    if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

pub fn generate(ds: &Dataset, store: &Store) -> Value {
    let hosts = store.hosts();
    let ctx = Ctx {
        multi_host: hosts.len() > 1,
        interval: interval_text(store.interval_ms),
    };
    let mut ids = Ids { next: 0 };

    let mut groups: Vec<(&str, Vec<&(&Activity, crate::table::Table)>)> = Vec::new();
    for entry in &ds.tables {
        match groups.iter_mut().find(|(g, _)| *g == entry.0.group) {
            Some((_, v)) => v.push(entry),
            None => groups.push((entry.0.group, vec![entry])),
        }
    }
    groups.sort_by_key(|(g, _)| {
        (
            GROUP_ORDER
                .iter()
                .position(|o| o == g)
                .unwrap_or(GROUP_ORDER.len()),
            g.to_string(),
        )
    });

    let mut variables = vec![variable("host", "label_values(host)")];
    let mut seen_dims: BTreeSet<&str> = BTreeSet::new();
    let mut panels: Vec<Value> = Vec::new();
    let mut y: u32 = 0;
    let mut shown: usize = 0;

    for (group, entries) in groups.iter() {
        let mut group_panels: Vec<Value> = Vec::new();
        for (activity, table) in entries {
            let panels = activity_panels(activity, &table.header, store, &mut ids, &ctx);
            if panels.is_empty() {
                continue;
            }
            if let Some(dim) = &activity.dimension {
                if seen_dims.insert(dim.label) {
                    let first = value_columns(activity, &table.header, store)[0];
                    let metric = metric_name(activity.label, first);
                    variables.push(variable(
                        dim.label,
                        &format!("label_values({metric}, {})", dim.label),
                    ));
                }
            }
            group_panels.extend(panels);
        }

        if group_panels.is_empty() {
            continue;
        }
        let row_y = y;
        y += 1;
        let collapsed = shown >= EXPANDED_GROUPS;
        shown += 1;
        let (w, h) = (12u32, 8u32);
        let mut placed = Vec::new();
        for (i, mut p) in group_panels.into_iter().enumerate() {
            let (col, row) = ((i % 2) as u32, (i / 2) as u32);
            p["gridPos"] = json!({"h": h, "w": w, "x": col * w, "y": y + row * h});
            placed.push(p);
        }
        let used_rows = (placed.len() as u32).div_ceil(2) * h;
        let mut row = json!({
            "id": ids.take(),
            "type": "row",
            "title": group,
            "collapsed": collapsed,
            "gridPos": {"h": 1, "w": 24, "x": 0, "y": row_y},
            "panels": []
        });
        if collapsed {
            row["panels"] = Value::Array(placed);
            panels.push(row);
        } else {
            panels.push(row);
            panels.extend(placed);
            y += used_rows;
        }
    }

    let title = match hosts.as_slice() {
        [h] => format!("SAR - {h}"),
        [] => "SAR".to_string(),
        many => format!("SAR - {} hosts", many.len()),
    };

    json!({
        "uid": DASHBOARD_UID,
        "title": title,
        "tags": ["sarda"],
        "timezone": "utc",
        "schemaVersion": 39,
        "version": 1,
        "editable": true,
        "graphTooltip": 1,
        "refresh": "",
        "time": {
            "from": timeutil::format_rfc3339(store.min_ts_ms / 1000),
            "to": timeutil::format_rfc3339(store.max_ts_ms / 1000 + 1)
        },
        "templating": {"list": variables},
        "annotations": {"list": []},
        "panels": panels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::activity;
    use crate::table::Table;

    fn dataset() -> Dataset {
        let cpu = Table::parse(
            "# hostname;interval;timestamp;CPU;%user;%idle\n\
             h;60;2026-11-25 10:00:00 UTC;-1;10.00;90.00\n\
             h;60;2026-11-25 10:00:00 UTC;0;10.00;90.00\n",
        )
        .unwrap()
        .0;
        let mem = Table::parse(
            "# hostname;interval;timestamp;kbmemfree;kbmemused;%memused;brandnew\n\
             h;60;2026-11-25 10:00:00 UTC;1;2;3;4\n",
        )
        .unwrap()
        .0;
        Dataset {
            tables: vec![
                (activity("per_cpu").unwrap(), cpu),
                (activity("memory").unwrap(), mem),
            ],
            warnings: vec![],
        }
    }

    fn all_panels(d: &Value) -> Vec<&Value> {
        let mut out = Vec::new();
        for p in d["panels"].as_array().unwrap() {
            if p["type"] == "row" {
                out.extend(p["panels"].as_array().unwrap());
            } else {
                out.push(p);
            }
        }
        out
    }

    #[test]
    fn generates_dimension_panels_with_variables() {
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        let names: Vec<&str> = d["templating"]["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["host", "cpu"]);
        let panels = all_panels(&d);
        let idle = panels
            .iter()
            .find(|p| p["title"] == "CPU utilization, per CPU: %idle")
            .unwrap();
        assert_eq!(
            idle["targets"][0]["expr"],
            "sarda_per_cpu_pct_idle{host=~\"$host\", cpu=~\"$cpu\"}"
        );
        assert_eq!(idle["fieldConfig"]["defaults"]["unit"], "percent");
        assert_eq!(idle["interval"], "1m");
    }

    fn dataset_with_swap() -> Dataset {
        use crate::table::Table;
        let mut ds = dataset();
        let swap = Table::parse(
            "# hostname;interval;timestamp;pswpin/s;pswpout/s\n\
             h;60;2026-11-25 10:00:00 UTC;0.00;0.00\n",
        )
        .unwrap()
        .0;
        ds.tables.push((activity("swap").unwrap(), swap));
        ds
    }

    #[test]
    fn zero_valued_activities_keep_their_panels() {
        let ds = dataset_with_swap();
        let store = Store::from_dataset(&ds);
        assert!(store.by_name("sarda_swap_pswpin_s").next().is_some());
        let d = generate(&ds, &store);
        let targets: Vec<&str> = all_panels(&d)
            .iter()
            .flat_map(|p| p["targets"].as_array().unwrap().iter())
            .map(|t| t["expr"].as_str().unwrap())
            .collect();
        assert!(
            targets.iter().any(|e| e.starts_with("sarda_swap_pswpin_s")),
            "a recorded activity gets its panel even when every value is zero"
        );
        let rows: Vec<&str> = d["panels"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["type"] == "row")
            .map(|p| p["title"].as_str().unwrap())
            .collect();
        assert!(rows.contains(&"Swap"));
    }

    #[test]
    fn activities_missing_from_the_files_get_no_panels() {
        // The fixture dataset has per_cpu and memory only.
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        let text = serde_json::to_string(&all_panels(&d)).unwrap();
        for absent in [
            "sarda_disk_",
            "sarda_swap_",
            "sarda_network_dev_",
            "sarda_tty_",
        ] {
            assert!(
                !text.contains(absent),
                "panel for unrecorded activity: {absent}"
            );
        }
        let rows: Vec<&str> = d["panels"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["type"] == "row")
            .map(|p| p["title"].as_str().unwrap())
            .collect();
        assert_eq!(rows, ["CPU", "Memory"]);
    }

    #[test]
    fn unknown_columns_are_not_dropped() {
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        let other = all_panels(&d)
            .into_iter()
            .find(|p| p["title"] == "Other")
            .unwrap();
        assert_eq!(
            other["targets"][0]["expr"],
            "sarda_memory_brandnew{host=~\"$host\"}"
        );
    }

    #[test]
    fn rows_are_ordered_and_later_ones_collapsed() {
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        let rows: Vec<&str> = d["panels"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["type"] == "row")
            .map(|p| p["title"].as_str().unwrap())
            .collect();
        assert_eq!(rows, ["CPU", "Memory"]);
        assert_eq!(d["panels"][0]["collapsed"], false);
    }

    #[test]
    fn panel_ids_are_unique_including_nested_ones() {
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        let mut ids = BTreeSet::new();
        for p in d["panels"].as_array().unwrap() {
            assert!(ids.insert(p["id"].as_u64().unwrap()));
            for nested in p["panels"].as_array().into_iter().flatten() {
                assert!(ids.insert(nested["id"].as_u64().unwrap()));
            }
        }
    }

    #[test]
    fn rare_variables_are_hidden() {
        assert_eq!(variable("cpu", "q")["hide"], 0);
        assert_eq!(variable("fan", "q")["hide"], 2);
    }

    #[test]
    fn time_range_matches_the_data() {
        let ds = dataset();
        let store = Store::from_dataset(&ds);
        let d = generate(&ds, &store);
        assert_eq!(d["time"]["from"], "2026-11-25T10:00:00Z");
    }

    #[test]
    fn ref_ids_roll_over() {
        assert_eq!(ref_id_letter(0), "A");
        assert_eq!(ref_id_letter(25), "Z");
        assert_eq!(ref_id_letter(26), "AA");
    }
}
