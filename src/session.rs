//! One sarDa run: convert, serve, start the Viewer, wait, clean up.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};

use crate::cli::{Cli, DEFAULT_PORT};
use crate::dashboard::{self, DASHBOARD_UID};
use crate::dataset::{self, Dataset};
use crate::error::{Error, Kind, Result};
use crate::runtime::{missing_image_help, Runtime};
use crate::sadf::{self, SadfOutput};
use crate::store::Store;
use crate::timeutil;
use crate::viewer::{self, Viewer, ViewerSpec, NAME_PREFIX};
use crate::{api, csvexport};

const CONTAINERFILE: &str = include_str!("../container/Containerfile.sysstat");
const PULL_TIMEOUT: Duration = Duration::from_secs(30);
const READY_TIMEOUT: Duration = Duration::from_secs(120);

static SERVING: AtomicBool = AtomicBool::new(false);
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn containerfile() -> &'static str {
    CONTAINERFILE
}

/// Progress goes to stderr; structured events go to stdout with `--json`.
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

fn install_signal_handler(tx: mpsc::Sender<()>) {
    let _ = ctrlc::set_handler(move || {
        if SERVING.load(Ordering::SeqCst) {
            let _ = tx.send(());
        } else {
            let pid = std::process::id();
            if let Some(rt) = RUNTIME.get() {
                rt.remove_container(&format!("sarda-sadf-{pid}"));
                rt.remove_container(&format!("{NAME_PREFIX}{pid}"));
            }
            let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("sarda-{pid}")));
            std::process::exit(130);
        }
    });
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
        if rt.pull(image, PULL_TIMEOUT) {
            return Ok(());
        }
    }
    Err(Error::new(
        Kind::MissingImage,
        missing_image_help(image, role, hint),
    ))
}

fn parse_window(cli: &Cli) -> Result<(Option<i64>, Option<i64>)> {
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

fn existing_files(cli: &Cli, rep: &Reporter) -> Result<Vec<PathBuf>> {
    if cli.files.is_empty() {
        return Err(Error::new(Kind::Usage, "no SA files given (see --help)"));
    }
    let mut files = Vec::new();
    for f in &cli.files {
        if f.is_file() {
            files.push(f.clone());
        } else {
            rep.warn(format!("{} is not a file; skipping", f.display()));
        }
    }
    if files.is_empty() {
        return Err(Error::new(Kind::Usage, "none of the given SA files exist"));
    }
    Ok(files)
}

fn summary(ds: &Dataset, store: &Store) -> Value {
    json!({
        "hosts": store.hosts(),
        "from": timeutil::format_rfc3339(store.min_ts_ms / 1000),
        "to": timeutil::format_rfc3339(store.max_ts_ms / 1000),
        "series": store.series.len(),
        "metrics": store.metric_names().len(),
        "activities": ds.tables.iter().map(|(a, _)| a.label).collect::<Vec<_>>(),
        "warnings": ds.warnings,
    })
}

fn wait_for_shutdown(rx: &mpsc::Receiver<()>, viewer: Option<&Viewer>) -> Result<()> {
    SERVING.store(true, Ordering::SeqCst);
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return Ok(()),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(v) = viewer {
                    if !v.is_running() {
                        return Err(Error::new(
                            Kind::ViewerFailed,
                            "the Grafana container stopped unexpectedly",
                        ));
                    }
                }
            }
        }
    }
}

pub fn run(cli: &Cli, rep: &Reporter) -> Result<()> {
    let (start, end) = parse_window(cli)?;
    let files = existing_files(cli, rep)?;
    let (tx, rx) = mpsc::channel();
    install_signal_handler(tx);

    let host_sadf = std::env::var_os("SARDA_SADF_HOST").is_some();
    let export_only = cli.no_grafana && cli.export_csv.is_some();
    let mut rt = None;
    if !host_sadf || !cli.no_grafana {
        let detected = Runtime::detect(cli.runtime)?;
        rep.debug(format!("using container runtime '{}'", detected.bin));
        let _ = RUNTIME.set(detected.clone());
        detected.remove_stale();
        if !host_sadf {
            ensure_image(
                &detected,
                &cli.sadf_image,
                "sysstat (sadf)",
                "Build it once on a machine with network access and load it here:\n  sarda --print-containerfile | podman build -t localhost/sarda-sysstat:latest -f - .\n  podman save -o sarda-sysstat.tar localhost/sarda-sysstat:latest   # then, on this host:\n  podman load -i sarda-sysstat.tar",
                cli.no_pull,
                rep,
            )?;
        }
        if !cli.no_grafana {
            ensure_image(
                &detected,
                &cli.grafana_image,
                "Grafana",
                &format!(
                    "Mirror it into your registry or load it from an archive:\n  podman pull {img} && podman save -o grafana.tar {img}   # on a connected machine\n  podman load -i grafana.tar                              # on this host",
                    img = cli.grafana_image
                ),
                cli.no_pull,
                rep,
            )?;
        }
        rt = Some(detected);
    }

    rep.info(format!(
        "Converting {} SA file(s) with sadf ...",
        files.len()
    ));
    let output: SadfOutput = if host_sadf {
        sadf::run_on_host(&files)?
    } else {
        sadf::run_in_container(
            rt.as_ref().expect("runtime detected"),
            &cli.sadf_image,
            &files,
            Duration::from_secs(cli.sadf_timeout),
        )?
    };
    let names: Vec<String> = files
        .iter()
        .map(|f| {
            f.file_name().map_or_else(
                || f.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let ds = dataset::assemble(&output, &names, start, end)?;
    for w in &ds.warnings {
        rep.warn(w);
    }
    let store = Arc::new(Store::from_dataset(&ds));
    let info = summary(&ds, &store);
    rep.info(format!(
        "Loaded {} series ({} metrics) from {} activities, {} .. {}",
        info["series"],
        info["metrics"],
        ds.tables.len(),
        info["from"].as_str().unwrap_or("?"),
        info["to"].as_str().unwrap_or("?")
    ));

    if let Some(dir) = &cli.export_csv {
        let count = csvexport::export(&ds, dir)?;
        rep.info(format!("Wrote {count} CSV file(s) to {}", dir.display()));
        rep.event(json!({"event": "exported", "dir": dir, "files": count}));
        if export_only {
            return Ok(());
        }
    }

    let api = api::serve(store.clone(), "127.0.0.1:0")?;
    let api_url = format!("http://{}", api.addr);
    rep.debug(format!("Sarda API listening on {api_url}"));

    if cli.no_grafana {
        rep.info(format!(
            "Sarda API ready on {api_url} (Prometheus-compatible; Ctrl-C to stop)"
        ));
        rep.result_line(&api_url);
        rep.event(json!({"event": "api_ready", "api_url": api_url, "data": info}));
        wait_for_shutdown(&rx, None)?;
        rep.event(json!({"event": "stopped"}));
        return Ok(());
    }

    let rt = rt.expect("runtime detected");
    let port = viewer::pick_port(cli.port.or(Some(DEFAULT_PORT)), cli.port.is_some())?;
    let dashboard = dashboard::generate(&ds, &store);
    rep.info(format!("Starting Grafana ({}) ...", cli.grafana_image));
    let viewer = Viewer::start(
        &rt,
        &ViewerSpec {
            image: &cli.grafana_image,
            port,
            api_addr: api.addr,
            scrape_interval: format!("{}s", (store.interval_ms / 1000).max(1)),
            dashboard: &dashboard,
        },
    )?;
    viewer.wait_ready(READY_TIMEOUT)?;

    let url = format!(
        "http://127.0.0.1:{port}/d/{DASHBOARD_UID}/sarda?orgId=1&from={}&to={}",
        store.min_ts_ms,
        store.max_ts_ms + 1000
    );
    rep.info(format!(
        "\nGrafana is ready. Open: {url}\nPress Ctrl-C to stop and clean up."
    ));
    rep.result_line(&url);
    rep.event(json!({
        "event": "ready",
        "url": url,
        "grafana_port": port,
        "api_url": api_url,
        "runtime": rt.bin,
        "container": viewer.container_name(),
        "data": info,
    }));

    let result = wait_for_shutdown(&rx, Some(&viewer));
    rep.info("Stopping Grafana and cleaning up ...");
    drop(viewer);
    rep.event(json!({"event": "stopped"}));
    result
}
