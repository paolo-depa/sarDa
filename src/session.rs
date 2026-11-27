//! One sarDa run: convert, serve, start Grafana, wait, clean up.

use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::api;
use crate::cli::{
    ApiArgs, EngineArgs, ExportCsvArgs, GrafanaArgs, InputArgs, OpenArgs, DEFAULT_PORT,
};
use crate::csvexport;
use crate::dashboard::{self, DASHBOARD_UID};
use crate::dataset::{self, Dataset};
use crate::error::{Error, Kind, Result};
use crate::grafana::{self, Grafana, GrafanaSpec};
use crate::inputs;
use crate::resources::{Handle, Resources};
use crate::runtime::{missing_image_help, Runtime};
use crate::sadf::{self, Input, SadfOutput, Verdict};
use crate::store::Store;
use crate::timeutil;

const CONTAINERFILE: &str = include_str!("../container/Containerfile.sysstat");
const READY_TIMEOUT: Duration = Duration::from_secs(120);

pub fn containerfile() -> &'static str {
    CONTAINERFILE
}

/// Inclusive date bounds of files that can be skipped without reading them.
/// A file's day comes from its own header (`sadf -j -H`), never from its name.
/// One day of slack on each side keeps the decision safe across the file's own
/// timezone: a file is only skipped when it cannot contribute a sample to the
/// Observation window.
fn skip_bounds(start: Option<i64>, end: Option<i64>) -> (Option<String>, Option<String>) {
    let day = 86_400;
    let fmt = |t: i64| timeutil::format_rfc3339(t)[..10].to_string();
    (start.map(|t| fmt(t - day)), end.map(|t| fmt(t + day)))
}

/// Progress goes to stderr; structured events go to stdout with `--json`. This
/// is the only code that writes a run's result to stdout (the `containerfile` and `catalog`
/// modes in `main.rs` aside), so callers can rely on stdout being only the
/// URL, or one JSON object per line.
pub struct Reporter {
    pub json: bool,
    pub verbose: bool,
}

impl Reporter {
    pub fn info(&self, msg: impl AsRef<str>) {
        eprintln!("{}", msg.as_ref());
    }

    pub fn warn(&self, msg: impl AsRef<str>) {
        eprintln!("warning: {}", msg.as_ref());
    }

    pub fn debug(&self, msg: impl AsRef<str>) {
        if self.verbose {
            eprintln!("debug: {}", msg.as_ref());
        }
    }

    pub fn event(&self, value: Value) {
        if self.json {
            println!("{value}");
        }
    }

    /// The one line a human-mode caller may want to capture from stdout.
    pub fn result_line(&self, line: &str) {
        if !self.json {
            println!("{line}");
        }
    }
}

fn ensure_image(
    rt: &Runtime,
    image: &str,
    role: &str,
    hint: &str,
    no_pull: bool,
    rep: &Reporter,
) -> Result<()> {
    if rt.image_exists(image) {
        return Ok(());
    }
    let pullable = !no_pull && !image.starts_with("localhost/");
    if pullable {
        rep.info(format!("Pulling {role} image {image} ..."));
        if rt.pull(image) {
            return Ok(());
        }
    }
    Err(Error::new(
        Kind::MissingImage,
        missing_image_help(image, role, hint),
    ))
}

fn parse_window(cli: &InputArgs) -> Result<(Option<i64>, Option<i64>)> {
    let parse = |v: &Option<String>| -> Result<Option<i64>> {
        v.as_deref()
            .map(|s| timeutil::parse_user(s).map_err(|e| Error::new(Kind::Usage, e)))
            .transpose()
    };
    let (start, end) = (parse(&cli.start)?, parse(&cli.end)?);
    if let (Some(s), Some(e)) = (start, end) {
        if s > e {
            return Err(Error::new(Kind::Usage, "--start is after --end"));
        }
    }
    Ok((start, end))
}

fn summary(ds: &Dataset, store: &Store, warnings: &[String]) -> Value {
    json!({
        "hosts": store.hosts(),
        "from": timeutil::format_rfc3339(store.min_ts_ms / 1000),
        "to": timeutil::format_rfc3339(store.max_ts_ms / 1000),
        "series": store.series.len(),
        "metrics": store.metric_names().len(),
        "activities": ds.tables.iter().map(|t| t.activity.label).collect::<Vec<_>>(),
        "warnings": warnings,
    })
}

fn wait_for_shutdown(
    resources: &Handle,
    rx: &mpsc::Receiver<()>,
    grafana: Option<&Grafana>,
) -> Result<()> {
    resources.mark_serving();
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(v) = grafana {
                    if !v.is_running() {
                        return Err(Error::new(
                            Kind::GrafanaFailed,
                            "the Grafana container stopped unexpectedly",
                        ));
                    }
                }
            }
        }
    }
}

fn sadf_image_hint(image: &str) -> String {
    format!(
        "On a machine with network access, run `sarda prepare --save sarda-images.tar`, then on this host `sarda load sarda-images.tar`.\nTo build {image} from the Containerfile instead: sarda prepare --build --sadf-image {image}"
    )
}

fn grafana_image_hint(image: &str) -> String {
    format!(
        "On a machine with network access, run `sarda prepare --save sarda-images.tar`, then on this host `sarda load sarda-images.tar`.\nOr mirror {image} into your registry and pass it with --grafana-image."
    )
}

fn ensure_images(
    engine: &EngineArgs,
    grafana_image: Option<&str>,
    rt: &Runtime,
    rep: &Reporter,
) -> Result<()> {
    ensure_image(
        rt,
        &engine.sadf_image,
        "sysstat (sadf)",
        &sadf_image_hint(&engine.sadf_image),
        engine.no_pull,
        rep,
    )?;
    if let Some(image) = grafana_image {
        ensure_image(
            rt,
            image,
            "Grafana",
            &grafana_image_hint(image),
            engine.no_pull,
            rep,
        )?;
    }
    Ok(())
}

/// Converts the files and settles each one's verdict. An incompatible file
/// stops the Session. Returns the stream with the warnings and the details
/// the verdicts produced.
fn convert(
    input: &InputArgs,
    engine: &EngineArgs,
    rt: &Runtime,
    inputs: &[Input],
    window: (Option<i64>, Option<i64>),
    resources: &Handle,
    rep: &Reporter,
) -> Result<(SadfOutput, Vec<String>, Vec<String>)> {
    rep.info(format!(
        "Converting {} SA file(s) with sadf ...",
        inputs.len()
    ));
    let bounds = skip_bounds(window.0, window.1);
    let skip = (bounds.0.as_deref(), bounds.1.as_deref());
    let output = sadf::run_in_container(
        rt,
        &engine.sadf_image,
        inputs,
        skip,
        Duration::from_secs(input.sadf_timeout),
        resources,
        |index| {
            if let (true, Some(input)) = (inputs.len() > 1, inputs.get(index)) {
                rep.info(format!("  [{}/{}] {}", index + 1, inputs.len(), input.name));
            }
        },
    )?;

    let verdicts = output.verdicts();
    let problems: Vec<String> = verdicts.iter().filter_map(Verdict::problem).collect();
    if !problems.is_empty() {
        return Err(Error::new(
            Kind::IncompatibleFile,
            format!(
                "{}\nUse an image whose sysstat is at least as new as the files (--sadf-image), or SA files written by an older sysstat.",
                problems.join("\n")
            ),
        ));
    }
    let warnings = verdicts.iter().filter_map(Verdict::warning).collect();
    let details: Vec<String> = verdicts.iter().filter_map(Verdict::detail).collect();
    for d in &details {
        rep.debug(d);
    }
    Ok((output, warnings, details))
}

/// Merges the converted files into one Dataset and one Store. Skipped files
/// explain "no data" as much as warnings do, so they are added to that error.
fn load(
    output: &SadfOutput,
    window: (Option<i64>, Option<i64>),
    mut warnings: Vec<String>,
    details: &[String],
    rep: &Reporter,
) -> Result<(Dataset, Arc<Store>, Value)> {
    let ds = match dataset::assemble(output, window.0, window.1) {
        Ok(ds) => ds,
        Err(mut e) => {
            for note in warnings.iter().chain(details).take(10) {
                e.message.push_str(&format!("\n  - {note}"));
            }
            return Err(e);
        }
    };
    warnings.extend(ds.warnings.iter().cloned());
    for w in &warnings {
        rep.warn(w);
    }
    let store = Arc::new(Store::from_dataset(&ds));
    let info = summary(&ds, &store, &warnings);
    rep.info(format!(
        "Loaded {} series ({} metrics) from {} activities, {} .. {}",
        info["series"],
        info["metrics"],
        ds.tables.len(),
        info["from"].as_str().unwrap_or("?"),
        info["to"].as_str().unwrap_or("?")
    ));
    Ok((ds, store, info))
}

fn export_csv(ds: &Dataset, dir: &Path, rep: &Reporter) -> Result<()> {
    let count = csvexport::export(ds, dir)?;
    rep.info(format!("Wrote {count} CSV file(s) to {}", dir.display()));
    rep.event(json!({"event": "exported", "dir": dir, "files": count}));
    Ok(())
}

/// Everything a command needs once the SA files have been read.
struct Loaded {
    rt: Runtime,
    resources: Resources,
    shutdown: mpsc::Receiver<()>,
    ds: Dataset,
    store: Arc<Store>,
    info: Value,
}

/// The shared start of every command that reads SA files: resolve the
/// inputs, make sure the images are there, convert, and load.
fn read_files(
    input: &InputArgs,
    engine: &EngineArgs,
    grafana_image: Option<&str>,
    rep: &Reporter,
) -> Result<Loaded> {
    let window = parse_window(input)?;
    let inputs = inputs::resolve(&input.files, rep)?;
    let rt = Runtime::detect(engine.runtime)?;
    rep.debug(format!("using container runtime '{}'", rt.bin));

    let resources = Resources::new(rt.clone());
    let (tx, shutdown) = mpsc::channel();
    resources.handle_signals(tx);
    resources.sweep_stale();

    ensure_images(engine, grafana_image, &rt, rep)?;
    let (output, warnings, details) =
        convert(input, engine, &rt, &inputs, window, &resources, rep)?;
    let (ds, store, info) = load(&output, window, warnings, &details, rep)?;
    Ok(Loaded {
        rt,
        resources,
        shutdown,
        ds,
        store,
        info,
    })
}

/// `sarda export-csv`: write the CSV files and exit.
pub fn export_csv_only(args: &ExportCsvArgs, rep: &Reporter) -> Result<()> {
    let loaded = read_files(&args.input, &args.engine, None, rep)?;
    export_csv(&loaded.ds, &args.dir, rep)
}

/// `sarda api`: serve only the Sarda API until Ctrl-C.
pub fn serve_api_only(args: &ApiArgs, rep: &Reporter) -> Result<()> {
    let loaded = read_files(&args.input, &args.engine, None, rep)?;
    let api = api::serve(loaded.store.clone(), "127.0.0.1:0")?;
    let api_url = format!("http://{}", api.addr);
    rep.debug(format!("Sarda API listening on {api_url}"));
    rep.info(format!(
        "Sarda API ready on {api_url} (Prometheus-compatible; Ctrl-C to stop)"
    ));
    rep.result_line(&api_url);
    rep.event(json!({"event": "api_ready", "api_url": api_url, "data": loaded.info}));
    wait_for_shutdown(&loaded.resources, &loaded.shutdown, None)?;
    rep.event(json!({"event": "stopped"}));
    Ok(())
}

/// `sarda open`: start the Grafana instance, announce the URL and wait for
/// Ctrl-C.
pub fn serve_grafana(args: &OpenArgs, rep: &Reporter) -> Result<()> {
    let grafana_args: &GrafanaArgs = &args.grafana;
    let loaded = read_files(
        &args.input,
        &args.engine,
        Some(&grafana_args.grafana_image),
        rep,
    )?;
    let Loaded {
        rt,
        resources,
        shutdown,
        ds,
        store,
        info,
    } = &loaded;
    let api = api::serve(store.clone(), "127.0.0.1:0")?;
    let api_url = format!("http://{}", api.addr);
    rep.debug(format!("Sarda API listening on {api_url}"));

    let port = grafana::pick_port(
        grafana_args.port.or(Some(DEFAULT_PORT)),
        grafana_args.port.is_some(),
    )?;
    let dashboard = dashboard::generate(ds, store);
    rep.info(format!(
        "Starting Grafana ({}) ...",
        grafana_args.grafana_image
    ));
    let grafana = Grafana::start(
        rt,
        &GrafanaSpec {
            image: &grafana_args.grafana_image,
            port,
            api_addr: api.addr,
            scrape_interval: format!("{}s", (store.interval_ms / 1000).max(1)),
            dashboard: &dashboard,
        },
        resources,
    )?;
    grafana.wait_ready(READY_TIMEOUT)?;

    let url = format!(
        "http://127.0.0.1:{port}/d/{DASHBOARD_UID}/sarda?orgId=1&from={}&to={}",
        store.min_ts_ms,
        store.max_ts_ms + 1000
    );
    rep.info(format!(
        "\nGrafana is ready. Open: {url}\nChanges made in Grafana are not saved. Press Ctrl-C to stop and clean up."
    ));
    rep.result_line(&url);
    rep.event(json!({
        "event": "ready",
        "url": url,
        "grafana_port": port,
        "api_url": api_url,
        "runtime": rt.bin,
        "container": grafana.container_name(),
        "data": info,
    }));
    if grafana_args.open {
        open_in_browser(&url, rep);
    }

    let result = wait_for_shutdown(resources, shutdown, Some(&grafana));
    rep.info("Stopping Grafana and cleaning up ...");
    resources.remove_all();
    rep.event(json!({"event": "stopped"}));
    result
}

fn open_in_browser(url: &str, rep: &Reporter) {
    let started = std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = started {
        rep.warn(format!("could not open a browser with xdg-open: {e}"));
    }
}
