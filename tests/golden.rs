use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sarda::api::{self, parse_query_string};
use sarda::csvexport;
use sarda::dashboard;
use sarda::dataset::{self, Dataset};
use sarda::promql::{self, Value};
use sarda::sadf;
use sarda::store::Store;
use sarda::timeutil;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn names() -> Vec<String> {
    vec!["sa20261125".into(), "sa20261126".into()]
}

fn load(start: Option<i64>, end: Option<i64>) -> Dataset {
    let out = sadf::read_stream_file(&fixtures().join("sadf.stream")).unwrap();
    dataset::assemble(&out, &names(), start, end).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sarda-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn cells_equal(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-9 * x.abs().max(1.0),
        _ => false,
    }
}

#[test]
fn csv_export_matches_the_python_golden_files() {
    let ds = load(None, None);
    let dir = scratch("golden");
    csvexport::export(&ds, &dir).unwrap();

    let list = |d: &Path| -> BTreeSet<String> {
        std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };
    let golden_dir = fixtures().join("golden");
    assert_eq!(
        list(&dir),
        list(&golden_dir),
        "different set of exported files"
    );

    for name in list(&golden_dir) {
        let want = std::fs::read_to_string(golden_dir.join(&name)).unwrap();
        let got = std::fs::read_to_string(dir.join(&name)).unwrap();
        let (want, got): (Vec<_>, Vec<_>) = (want.lines().collect(), got.lines().collect());
        assert_eq!(want.len(), got.len(), "{name}: line count");
        for (i, (w, g)) in want.iter().zip(&got).enumerate() {
            let (w, g): (Vec<_>, Vec<_>) = (w.split(';').collect(), g.split(';').collect());
            assert_eq!(w.len(), g.len(), "{name}:{}: cell count", i + 1);
            for (a, b) in w.iter().zip(&g) {
                assert!(cells_equal(a, b), "{name}:{}: {a:?} != {b:?}", i + 1);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn time_window_is_applied_in_utc() {
    let all = load(None, None);
    let store = Store::from_dataset(&all);
    let first = store.min_ts_ms / 1000;
    let last = store.max_ts_ms / 1000;
    assert!(last > first + 2, "fixture should span several seconds");

    let narrowed = load(Some(first + 2), Some(last - 1));
    let nstore = Store::from_dataset(&narrowed);
    assert!(nstore.min_ts_ms / 1000 >= first + 2);
    assert!(nstore.max_ts_ms / 1000 < last);
    assert!(nstore.series.len() <= store.series.len());

    let out = sadf::read_stream_file(&fixtures().join("sadf.stream")).unwrap();
    let nothing = timeutil::parse_user("2000-01-01T00:00:00Z").unwrap();
    let err = dataset::assemble(&out, &names(), None, Some(nothing)).unwrap_err();
    assert_eq!(err.kind, sarda::error::Kind::NoData);
    assert!(err.message.contains("--start/--end"));
}

#[test]
fn store_exposes_labelled_series() {
    let store = Store::from_dataset(&load(None, None));
    let idle: Vec<_> = store.by_name("sarda_per_cpu_pct_idle").collect();
    assert!(idle.len() >= 2);
    assert!(idle
        .iter()
        .any(|s| s.labels.get("cpu").map(String::as_str) == Some("all")));
    assert!(idle.iter().all(|s| s.labels.contains_key("host")));
    assert!(store
        .by_name("sarda_network_dev_rxkB_s")
        .any(|s| s.labels.contains_key("iface")));
    assert!(store.by_name("sarda_memory_kbmemfree").next().is_some());
    for s in &store.series {
        assert!(
            s.samples.windows(2).all(|w| w[0].0 < w[1].0),
            "{} not strictly sorted",
            s.name
        );
    }
}

#[test]
fn every_dashboard_query_parses_and_returns_data() {
    let ds = load(None, None);
    let store = Store::from_dataset(&ds);
    let dash = dashboard::generate(&ds, &store);

    let mut exprs = Vec::new();
    let mut ids = BTreeSet::new();
    let mut collect = |p: &serde_json::Value| {
        assert!(ids.insert(p["id"].as_u64().unwrap()), "duplicate panel id");
        for t in p["targets"].as_array().into_iter().flatten() {
            exprs.push(t["expr"].as_str().unwrap().to_string());
        }
    };
    for p in dash["panels"].as_array().unwrap() {
        collect(p);
        for nested in p["panels"].as_array().into_iter().flatten() {
            collect(nested);
        }
    }
    assert!(exprs.len() > 50, "only {} queries generated", exprs.len());

    // Panels follow what was collected, not what the values are: an Activity
    // present in the files has panels even if every sample is zero, and an
    // Activity absent from them has none.
    let metrics = store.metric_names();
    let panel_metrics: BTreeSet<String> = exprs
        .iter()
        .map(|e| e.split('{').next().unwrap().to_string())
        .collect();
    let present: BTreeSet<&str> = ds.tables.iter().map(|(a, _)| a.label).collect();
    for activity in sarda::catalog::ACTIVITIES {
        let prefix = format!("sarda_{}_", activity.label);
        let recorded = metrics.iter().any(|m| m.starts_with(&prefix));
        let has_panel = panel_metrics.iter().any(|m| m.starts_with(&prefix));
        assert_eq!(
            has_panel, recorded,
            "activity {}: recorded in the files = {recorded}, dashboards it = {has_panel}",
            activity.label
        );
        if !present.contains(activity.label) {
            assert!(
                !has_panel,
                "activity {} is missing from the files",
                activity.label
            );
        }
    }
    // Swap was recorded and is flat zero on this host: it keeps its panel.
    let swap: Vec<&sarda::store::Series> = store.by_name("sarda_swap_pswpin_s").collect();
    assert!(!swap.is_empty());
    assert!(swap
        .iter()
        .all(|s| s.samples.iter().all(|(_, v)| *v == 0.0)));
    assert!(panel_metrics.contains("sarda_swap_pswpin_s"));

    let variables: Vec<String> = dash["templating"]["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| format!("${}", v["name"].as_str().unwrap()))
        .collect();
    let steps: Vec<i64> = vec![store.min_ts_ms, store.max_ts_ms];
    for e in exprs {
        let mut query = e.clone();
        for v in &variables {
            query = query.replace(v.as_str(), ".*");
        }
        assert!(!query.contains('$'), "undeclared variable in {e}");
        let expr = promql::parse(&query).unwrap_or_else(|err| panic!("{e}: {err}"));
        match promql::eval(&expr, &store, &steps, 300_000).unwrap() {
            Value::Vector(v) => assert!(!v.is_empty(), "no data for {e}"),
            Value::Scalar(_) => panic!("scalar for {e}"),
        }
    }
}

#[test]
fn api_serves_the_fixture_end_to_end() {
    let store = Arc::new(Store::from_dataset(&load(None, None)));
    let (from, to) = (store.min_ts_ms / 1000, store.max_ts_ms / 1000);
    let qs = format!(
        "query=sum%20by%20(host)%20(sarda_per_cpu_pct_idle%7Bcpu%21%3D%22all%22%7D)&start={from}&end={to}&step=1"
    );
    let resp = api::handle(
        &store,
        "GET",
        "/api/v1/query_range",
        &parse_query_string(&qs),
    );
    assert_eq!(resp.status, 200, "{}", resp.body);
    let json: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(json["data"]["resultType"], "matrix");
    let values = json["data"]["result"][0]["values"].as_array().unwrap();
    assert!(values.len() >= 3);
}
