//! The Sarda API: the slice of the Prometheus HTTP API that Grafana's
//! built-in Prometheus datasource needs, served from an in-memory [`Store`].
//!
//! [`handle`] is a pure function from request to response so it can be tested
//! without sockets; [`serve`] wraps it in a small multi-threaded HTTP server.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::{json, Value as Json};

use crate::error::{Error, Result};
use crate::promql::{self, Evaluated, Selector, Value};
use crate::store::Store;
use crate::timeutil;

const MAX_POINTS: i64 = 11_000;
const MIN_LOOKBACK_MS: i64 = 300_000;
const WORKERS: usize = 8;

pub type Params = Vec<(String, String)>;

#[derive(Debug)]
pub struct ApiResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

fn ok(data: Json) -> ApiResponse {
    ApiResponse {
        status: 200,
        content_type: "application/json",
        body: json!({"status": "success", "data": data}).to_string(),
    }
}

fn err(status: u16, error_type: &str, msg: &str) -> ApiResponse {
    ApiResponse {
        status,
        content_type: "application/json",
        body: json!({"status": "error", "errorType": error_type, "error": msg}).to_string(),
    }
}

fn bad(msg: &str) -> ApiResponse {
    err(400, "bad_data", msg)
}

fn param<'a>(params: &'a Params, name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn format_value(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 {
            "+Inf".into()
        } else {
            "-Inf".into()
        }
    } else {
        format!("{v}")
    }
}

fn labels_json(e: &Evaluated) -> Json {
    serde_json::to_value(&e.labels).unwrap_or(Json::Null)
}

/// Prometheus timestamps: unix seconds (possibly fractional) or RFC 3339.
fn parse_time_ms(s: &str) -> Option<i64> {
    if let Ok(f) = s.parse::<f64>() {
        return Some((f * 1000.0).round() as i64);
    }
    timeutil::parse_user(s).ok().map(|t| t * 1000)
}

/// Prometheus durations: plain seconds or `15s`, `1m`, `2h`, `1d`, `1w`.
fn parse_step_ms(s: &str) -> Option<i64> {
    if let Ok(f) = s.parse::<f64>() {
        return Some((f * 1000.0).round() as i64);
    }
    let split = s.find(|c: char| !c.is_ascii_digit() && c != '.')?;
    let n: f64 = s[..split].parse().ok()?;
    let unit = match &s[split..] {
        "ms" => 1.0,
        "s" => 1_000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        "d" => 86_400_000.0,
        "w" => 604_800_000.0,
        _ => return None,
    };
    Some((n * unit).round() as i64)
}

fn lookback_ms(store: &Store) -> i64 {
    MIN_LOOKBACK_MS.max(store.interval_ms * 2)
}

fn selectors(params: &Params) -> std::result::Result<Vec<Selector>, String> {
    params
        .iter()
        .filter(|(k, _)| k == "match[]" || k == "match")
        .map(|(_, v)| promql::parse_selector(v))
        .collect()
}

fn matching<'a>(store: &'a Store, sels: &'a [Selector]) -> Vec<&'a crate::store::Series> {
    if sels.is_empty() {
        store.series.iter().collect()
    } else {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for sel in sels {
            for s in sel.select(store) {
                if seen.insert(s as *const _ as usize) {
                    out.push(s);
                }
            }
        }
        out
    }
}

fn query_range(store: &Store, params: &Params) -> ApiResponse {
    let Some(q) = param(params, "query") else {
        return bad("missing 'query' parameter");
    };
    let (Some(start), Some(end), Some(step)) = (
        param(params, "start").and_then(parse_time_ms),
        param(params, "end").and_then(parse_time_ms),
        param(params, "step").and_then(parse_step_ms),
    ) else {
        return bad("'start', 'end' and 'step' are required and must be valid");
    };
    if step <= 0 {
        return bad("zero or negative query resolution step widths are not accepted");
    }
    if end < start {
        return bad("end timestamp must not be before start time");
    }
    if (end - start) / step > MAX_POINTS {
        return bad("exceeded maximum resolution of 11,000 points per timeseries. Try decreasing the query resolution (?step=XX)");
    }
    let expr = match promql::parse(q) {
        Ok(e) => e,
        Err(e) => return bad(&format!("invalid parameter \"query\": {e}")),
    };
    let steps: Vec<i64> = (0..=((end - start) / step))
        .map(|i| start + i * step)
        .collect();
    match promql::eval(&expr, store, &steps, lookback_ms(store)) {
        Ok(Value::Vector(mut vec)) => {
            vec.sort_by(|a, b| a.labels.cmp(&b.labels));
            let result: Vec<Json> = vec
                .iter()
                .map(|e| {
                    let values: Vec<Json> = e
                        .points
                        .iter()
                        .map(|(t, v)| json!([*t as f64 / 1000.0, format_value(*v)]))
                        .collect();
                    json!({"metric": labels_json(e), "values": values})
                })
                .collect();
            ok(json!({"resultType": "matrix", "result": result}))
        }
        Ok(Value::Scalar(_)) => err(
            400,
            "bad_data",
            "invalid expression type \"scalar\" for range query, must be Instant vector or Range vector",
        ),
        Err(e) => err(422, "execution", &e),
    }
}

fn query_instant(store: &Store, params: &Params) -> ApiResponse {
    let Some(q) = param(params, "query") else {
        return bad("missing 'query' parameter");
    };
    let t = param(params, "time")
        .and_then(parse_time_ms)
        .unwrap_or(store.max_ts_ms);
    let expr = match promql::parse(q) {
        Ok(e) => e,
        Err(e) => return bad(&format!("invalid parameter \"query\": {e}")),
    };
    match promql::eval(&expr, store, &[t], lookback_ms(store)) {
        Ok(Value::Scalar(v)) => ok(json!({
            "resultType": "scalar",
            "result": [t as f64 / 1000.0, format_value(v)]
        })),
        Ok(Value::Vector(mut vec)) => {
            vec.sort_by(|a, b| a.labels.cmp(&b.labels));
            let result: Vec<Json> = vec
                .iter()
                .filter_map(|e| {
                    e.points
                        .first()
                        .map(|(_, v)| json!({"metric": labels_json(e), "value": [t as f64 / 1000.0, format_value(*v)]}))
                })
                .collect();
            ok(json!({"resultType": "vector", "result": result}))
        }
        Err(e) => err(422, "execution", &e),
    }
}

fn labels(store: &Store, params: &Params) -> ApiResponse {
    let sels = match selectors(params) {
        Ok(s) => s,
        Err(e) => return bad(&e),
    };
    let mut names: BTreeSet<&str> = BTreeSet::new();
    names.insert("__name__");
    for s in matching(store, &sels) {
        names.extend(s.labels.keys().map(String::as_str));
    }
    ok(json!(names))
}

fn label_values(store: &Store, label: &str, params: &Params) -> ApiResponse {
    let sels = match selectors(params) {
        Ok(s) => s,
        Err(e) => return bad(&e),
    };
    let values: BTreeSet<&str> = matching(store, &sels)
        .into_iter()
        .filter_map(|s| {
            if label == "__name__" {
                Some(s.name.as_str())
            } else {
                s.labels.get(label).map(String::as_str)
            }
        })
        .collect();
    ok(json!(values))
}

fn series(store: &Store, params: &Params) -> ApiResponse {
    let sels = match selectors(params) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return bad("no match[] parameter provided"),
        Err(e) => return bad(&e),
    };
    let result: Vec<Json> = matching(store, &sels)
        .into_iter()
        .map(|s| {
            let mut m = s.labels.clone();
            m.insert("__name__".into(), s.name.clone());
            serde_json::to_value(m).unwrap_or(Json::Null)
        })
        .collect();
    ok(json!(result))
}

fn metadata(store: &Store) -> ApiResponse {
    let data: serde_json::Map<String, Json> = store
        .metric_names()
        .into_iter()
        .map(|n| {
            (
                n.to_string(),
                json!([{"type": "gauge", "help": "", "unit": ""}]),
            )
        })
        .collect();
    ok(Json::Object(data))
}

pub fn handle(store: &Store, method: &str, path: &str, params: &Params) -> ApiResponse {
    if method != "GET" && method != "POST" {
        return err(405, "bad_data", "method not allowed");
    }
    match path {
        "/api/v1/query_range" => query_range(store, params),
        "/api/v1/query" => query_instant(store, params),
        "/api/v1/labels" => labels(store, params),
        "/api/v1/series" => series(store, params),
        "/api/v1/metadata" => metadata(store),
        "/api/v1/query_exemplars" => ok(json!([])),
        "/api/v1/rules" => ok(json!({"groups": []})),
        "/api/v1/alerts" => ok(json!({"alerts": []})),
        "/api/v1/status/buildinfo" => ok(json!({
            "version": "2.45.0",
            "revision": "sarda",
            "branch": "sarda",
            "buildUser": "sarda",
            "buildDate": "",
            "goVersion": "n/a"
        })),
        "/-/healthy" | "/-/ready" => ApiResponse {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body: "Sarda API is ready.\n".into(),
        },
        p => {
            if let Some(name) = p
                .strip_prefix("/api/v1/label/")
                .and_then(|r| r.strip_suffix("/values"))
            {
                label_values(store, &percent_decode(name), params)
            } else {
                err(404, "bad_data", &format!("unsupported endpoint {p}"))
            }
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_query_string(qs: &str) -> Params {
    qs.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(p), String::new()),
        })
        .collect()
}

pub struct ApiServer {
    pub addr: SocketAddr,
}

/// Starts the API on `bind` (use port 0 for any free port) with worker
/// threads that live for the rest of the process.
pub fn serve(store: Arc<Store>, bind: &str) -> Result<ApiServer> {
    let server = tiny_http::Server::http(bind).map_err(|e| {
        Error::new(
            crate::error::Kind::PortUnavailable,
            format!("cannot bind {bind}: {e}"),
        )
    })?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| Error::other("the API did not bind to an IP address"))?;
    let server = Arc::new(server);
    for _ in 0..WORKERS {
        let (server, store) = (server.clone(), store.clone());
        std::thread::spawn(move || {
            while let Ok(mut request) = server.recv() {
                let method = request.method().as_str().to_string();
                let url = request.url().to_string();
                let (path, qs) = url.split_once('?').unwrap_or((url.as_str(), ""));
                let mut params = parse_query_string(qs);
                if method == "POST" {
                    let mut body = String::new();
                    let _ = request.as_reader().read_to_string(&mut body);
                    params.extend(parse_query_string(&body));
                }
                let resp = handle(&store, &method, path, &params);
                let mut response =
                    tiny_http::Response::from_string(resp.body).with_status_code(resp.status);
                if let Ok(h) = tiny_http::Header::from_bytes("Content-Type", resp.content_type) {
                    response = response.with_header(h);
                }
                let _ = request.respond(response);
            }
        });
    }
    Ok(ApiServer { addr })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Labels, Series};
    use std::io::Read;

    fn store() -> Store {
        let mk = |name: &str, host: &str, cpu: &str, vals: &[(i64, f64)]| {
            let mut labels = Labels::new();
            labels.insert("host".into(), host.into());
            labels.insert("cpu".into(), cpu.into());
            Series {
                name: name.into(),
                labels,
                samples: vals.to_vec(),
            }
        };
        Store::from_series(vec![
            mk(
                "sarda_per_cpu_pct_idle",
                "h1",
                "0",
                &[(0, 90.0), (60_000, 80.0)],
            ),
            mk(
                "sarda_per_cpu_pct_idle",
                "h1",
                "1",
                &[(0, 70.0), (60_000, 60.0)],
            ),
            mk("sarda_queue_runq_sz", "h1", "", &[(0, 3.0)]),
        ])
    }

    fn call(path: &str, qs: &str) -> (u16, Json) {
        let r = handle(&store(), "GET", path, &parse_query_string(qs));
        (r.status, serde_json::from_str(&r.body).unwrap())
    }

    #[test]
    fn query_range_returns_a_matrix() {
        let (s, j) = call(
            "/api/v1/query_range",
            "query=sarda_per_cpu_pct_idle%7Bcpu%3D%220%22%7D&start=0&end=60&step=60",
        );
        assert_eq!(s, 200);
        assert_eq!(j["data"]["resultType"], "matrix");
        let r = &j["data"]["result"][0];
        assert_eq!(r["metric"]["cpu"], "0");
        assert_eq!(r["metric"]["__name__"], "sarda_per_cpu_pct_idle");
        assert_eq!(r["values"][1], json!([60.0, "80"]));
    }

    #[test]
    fn query_range_accepts_duration_steps_and_aggregation() {
        let (s, j) = call(
            "/api/v1/query_range",
            "query=sum(sarda_per_cpu_pct_idle)&start=0&end=60&step=1m",
        );
        assert_eq!(s, 200);
        assert_eq!(j["data"]["result"][0]["values"][0], json!([0.0, "160"]));
    }

    #[test]
    fn health_check_scalar_query() {
        let (s, j) = call("/api/v1/query", "query=1%2B1");
        assert_eq!(s, 200);
        assert_eq!(j["data"]["resultType"], "scalar");
        assert_eq!(j["data"]["result"][1], "2");
    }

    #[test]
    fn instant_query_defaults_to_the_end_of_the_data() {
        let (_, j) = call("/api/v1/query", "query=sarda_per_cpu_pct_idle");
        assert_eq!(j["data"]["resultType"], "vector");
        assert_eq!(j["data"]["result"][0]["value"][1], "80");
    }

    #[test]
    fn label_values_support_match() {
        let (_, j) = call(
            "/api/v1/label/cpu/values",
            "match%5B%5D=sarda_per_cpu_pct_idle",
        );
        assert_eq!(j["data"], json!(["0", "1"]));
        let (_, j) = call("/api/v1/label/__name__/values", "");
        assert_eq!(
            j["data"],
            json!(["sarda_per_cpu_pct_idle", "sarda_queue_runq_sz"])
        );
        let (_, j) = call("/api/v1/label/host/values", "");
        assert_eq!(j["data"], json!(["h1"]));
    }

    #[test]
    fn labels_and_series() {
        let (_, j) = call("/api/v1/labels", "");
        assert_eq!(j["data"], json!(["__name__", "cpu", "host"]));
        let (s, _) = call("/api/v1/series", "");
        assert_eq!(s, 400);
        let (_, j) = call("/api/v1/series", "match%5B%5D=sarda_queue_runq_sz");
        assert_eq!(j["data"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn errors_follow_the_prometheus_shape() {
        let (s, j) = call(
            "/api/v1/query_range",
            "query=rate(x%5B5m%5D)&start=0&end=60&step=15",
        );
        assert_eq!(s, 400);
        assert_eq!(j["status"], "error");
        assert_eq!(j["errorType"], "bad_data");
        assert!(j["error"].as_str().unwrap().contains("not supported"));
        let (s, _) = call("/api/v1/nope", "");
        assert_eq!(s, 404);
        let (s, _) = call("/api/v1/query_range", "query=x&start=0&end=1000000&step=1");
        assert_eq!(s, 400);
    }

    #[test]
    fn buildinfo_and_metadata_exist_for_grafana() {
        let (s, j) = call("/api/v1/status/buildinfo", "");
        assert_eq!(s, 200);
        assert!(j["data"]["version"].is_string());
        let (_, j) = call("/api/v1/metadata", "");
        assert!(j["data"]["sarda_queue_runq_sz"].is_array());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b+c%2B"), "a b c+");
        assert_eq!(percent_decode("100%"), "100%");
        let p = parse_query_string("match%5B%5D=x&match%5B%5D=y");
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn server_answers_over_http() {
        let api = serve(Arc::new(store()), "127.0.0.1:0").unwrap();
        let mut stream = std::net::TcpStream::connect(api.addr).unwrap();
        use std::io::Write;
        write!(
            stream,
            "GET /api/v1/label/host/values HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.contains(r#""data":["h1"]"#), "{text}");
    }
}
