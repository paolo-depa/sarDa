//! Black-box tests of the `sarda` executable. Container runtimes are replaced
//! by small shell scripts placed first on PATH, so no real podman, docker or
//! image is needed.

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[path = "../src/testutil.rs"]
mod testutil;
use testutil::Scratch;

const BIN: &str = env!("CARGO_BIN_EXE_sarda");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn scratch(name: &str) -> Scratch {
    Scratch::new(name)
}

fn fake_runtime(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nPATH=/usr/bin:/bin\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fake runtime whose `run` prints the given sadf stream.
fn runtime_with_stream(dir: &Path, name: &str, stream: &Path) {
    fake_runtime(
        dir,
        name,
        &format!(
            "case \"$1\" in\n info|image|ps|rm|container) exit 0;;\n run) cat '{}';;\n *) exit 0;;\nesac",
            stream.display()
        ),
    );
}

/// A fake runtime whose `run` prints the recorded sadf stream.
fn working_runtime(dir: &Path, name: &str) {
    runtime_with_stream(dir, name, &fixtures().join("sadf.stream"));
}

fn working_podman(dir: &Path) {
    working_runtime(dir, "podman");
}

fn sarda(path_dir: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env("PATH", path_dir)
        .env_remove("SARDA_RUNTIME")
        .env_remove("SARDA_GRAFANA_IMAGE")
        .env_remove("SARDA_SADF_IMAGE");
    c
}

fn sa_files() -> Vec<PathBuf> {
    vec![
        fixtures().join("sa/sa20261125"),
        fixtures().join("sa/sa20261126"),
    ]
}

#[test]
fn no_arguments_is_a_usage_error() {
    let out = sarda(&scratch("usage")).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no SA files"));
}

#[test]
fn invalid_time_is_a_usage_error() {
    let dir = scratch("badtime");
    working_podman(&dir);
    let out = sarda(&dir)
        .args(["--start", "yesterday"])
        .args(sa_files())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("RFC 3339"));
}

#[test]
fn print_modes_need_no_runtime() {
    let empty = scratch("print");
    let out = sarda(&empty).arg("--print-containerfile").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("FROM ") && text.contains("sysstat"));

    let out = sarda(&empty).arg("--print-catalog").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("| Dashboard row |"));
}

#[test]
fn missing_runtime_exits_3_with_a_json_error() {
    let empty = scratch("noruntime");
    let out = sarda(&empty)
        .arg("--json")
        .args(sa_files())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let event: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&out.stdout).lines().last().unwrap()).unwrap();
    assert_eq!(event["event"], "error");
    assert_eq!(event["kind"], "no_runtime");
    assert_eq!(event["exit_code"], 3);
    assert!(String::from_utf8_lossy(&out.stderr).contains("podman or docker"));
}

#[test]
fn a_stopped_podman_falls_back_to_docker() {
    let dir = scratch("fallback");
    fake_runtime(&dir, "podman", "exit 1");
    working_runtime(&dir, "docker");
    let csv = dir.join("csv");
    let out = sarda(&dir)
        .args(["--no-grafana", "--export-csv"])
        .arg(&csv)
        .args(sa_files())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn missing_images_exit_4_with_instructions() {
    let dir = scratch("noimage");
    fake_runtime(
        &dir,
        "podman",
        "case \"$1\" in info) exit 0;; image) exit 1;; pull) exit 1;; *) exit 0;; esac",
    );
    let out = sarda(&dir).args(sa_files()).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("localhost/sarda-sysstat:latest"), "{err}");
    assert!(err.contains("--print-containerfile"), "{err}");
}

#[test]
fn export_csv_runs_the_whole_pipeline_with_a_fake_runtime() {
    let dir = scratch("export");
    working_podman(&dir);
    let csv = dir.join("csv");
    let out = sarda(&dir)
        .args(["--no-grafana", "--json", "--export-csv"])
        .arg(&csv)
        .args(sa_files())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(csv.join("per_cpu.csv").is_file());
    assert!(csv.join("per_cpu__idle.csv").is_file());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let event: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(event["event"], "exported");
    assert_eq!(event["files"], 67);
}

/// Builds a stream where the first file cannot be read and the second one is
/// the recorded fixture's second file.
fn stream_with_first_file_unreadable(dir: &Path) -> PathBuf {
    let fixture = std::fs::read_to_string(fixtures().join("sadf.stream")).unwrap();
    let tail = &fixture[fixture
        .find("@@FILE 1")
        .expect("second file in the fixture")..];
    let stream = dir.join("stream");
    std::fs::write(
        &stream,
        format!(
            "@@VERSION sysstat version 12.7.9\n@@FILE 0\n@@PROBE 0 3\n@@PERR Invalid system activity file: /in/0\n@@SKIP 0\n{tail}"
        ),
    )
    .unwrap();
    stream
}

#[test]
fn an_unreadable_file_aborts_the_run() {
    let dir = scratch("onebad");
    let stream = stream_with_first_file_unreadable(&dir);
    runtime_with_stream(&dir, "podman", &stream);
    let csv = dir.join("csv");
    let out = sarda(&dir)
        .args(["--no-grafana", "--export-csv"])
        .arg(&csv)
        .args(sa_files())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(8));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sa20261125: sadf cannot read this file"),
        "{err}"
    );
    assert!(err.contains("Invalid system activity file"), "{err}");
    assert!(!csv.exists(), "nothing is exported from an aborted run");
}

#[test]
fn every_unreadable_file_is_named() {
    let dir = scratch("allbad");
    let stream = dir.join("stream");
    std::fs::write(
        &stream,
        "@@VERSION sysstat version 12.7.9\n@@FILE 0\n@@PROBE 0 3\n@@PERR Invalid system activity file: /in/0\n@@SKIP 0\n\
         @@FILE 1\n@@PROBE 1 3\n@@PERR Invalid system activity file: /in/1\n@@SKIP 1\n",
    )
    .unwrap();
    runtime_with_stream(&dir, "podman", &stream);
    let out = sarda(&dir)
        .args(["--no-grafana"])
        .args(sa_files())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(8));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sa20261125: sadf cannot read this file"),
        "{err}"
    );
    assert!(
        err.contains("sa20261126: sadf cannot read this file"),
        "{err}"
    );
}

#[test]
fn a_file_from_a_newer_sysstat_is_refused() {
    let dir = scratch("newer");
    // Same SA file, but claiming sysstat 12.9.0 (the stream offers 12.7.9).
    let newer = dir.join("sa-newer");
    let mut bytes = std::fs::read(fixtures().join("sa/sa20261125")).unwrap();
    bytes[4..7].copy_from_slice(&[12, 9, 0]);
    std::fs::write(&newer, bytes).unwrap();
    working_podman(&dir);
    let out = sarda(&dir)
        .args(["--no-grafana"])
        .arg(&newer)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(8));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("written by sysstat 12.9.0"), "{err}");
    assert!(err.contains("provides 12.7.9"), "{err}");
    assert!(err.contains("--sadf-image"), "{err}");
}

#[test]
fn empty_window_exits_5() {
    let dir = scratch("window");
    working_podman(&dir);
    let out = sarda(&dir)
        .args(["--no-grafana", "--end", "2000-01-01T00:00:00Z"])
        .args(sa_files())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--start/--end"));
}

#[test]
fn missing_sa_files_are_skipped_but_not_all() {
    let dir = scratch("missing");
    working_podman(&dir);
    let csv = dir.join("csv");
    let mut files = sa_files();
    files.push(dir.join("does-not-exist"));
    let out = sarda(&dir)
        .args(["--no-grafana", "--export-csv"])
        .arg(&csv)
        .args(&files)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does-not-exist"));

    let out = sarda(&dir)
        .arg("--no-grafana")
        .arg(dir.join("nope"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

fn wait_for(path: &Path) {
    for _ in 0..500 {
        if path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {}", path.display());
}

fn terminate(child: &mut Child) -> Option<i32> {
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    child.wait().unwrap().code()
}

fn rm_calls(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn sigterm_while_converting_removes_the_parser_container() {
    let dir = scratch("sigterm-sadf");
    let (log, started) = (dir.join("rm.log"), dir.join("started"));
    fake_runtime(
        &dir,
        "podman",
        &format!(
            "case \"$1\" in\n rm) echo \"$@\" >> '{}';;\n run) touch '{}'; sleep 5;;\nesac",
            log.display(),
            started.display()
        ),
    );
    let mut child = sarda(&dir)
        .arg("--no-grafana")
        .args(sa_files())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&started);
    let pid = child.id();

    assert_eq!(terminate(&mut child), Some(130));
    assert_eq!(rm_calls(&log), [format!("rm -f sarda-{pid}-sadf")]);
}

#[test]
fn sigterm_while_grafana_starts_removes_its_container_and_temp_dir() {
    let dir = scratch("sigterm-grafana");
    let (log, started) = (dir.join("rm.log"), dir.join("started"));
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    // `run -d` is Grafana (and never becomes healthy); any other `run` is sadf.
    fake_runtime(
        &dir,
        "podman",
        &format!(
            "case \"$1\" in\n container) echo true;;\n rm) echo \"$@\" >> '{log}';;\n run) case \"$2\" in -d) touch '{started}';; *) cat '{stream}';; esac;;\nesac",
            log = log.display(),
            started = started.display(),
            stream = fixtures().join("sadf.stream").display()
        ),
    );
    let mut child = sarda(&dir)
        .env("TMPDIR", &tmp)
        .args(["--port", "0"])
        .args(sa_files())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for(&started);
    let pid = child.id();
    let session_dir = tmp.join(format!("sarda-{pid}"));
    assert!(
        session_dir.join("provisioning").is_dir(),
        "created before the signal"
    );

    assert_eq!(terminate(&mut child), Some(130));
    assert_eq!(rm_calls(&log), [format!("rm -f sarda-{pid}-grafana")]);
    assert!(!session_dir.exists(), "temp dir should be removed");
}

fn http_get(port: u16, path: &str) -> String {
    sarda::grafana::http_get(port, path, std::time::Duration::from_secs(5))
        .map(|(_, body)| body)
        .unwrap_or_default()
}

#[test]
fn file_warnings_reach_stderr_and_the_json_events() {
    let dir = scratch("verdict-warning");
    // The recorded stream, but as if the first file had needed `sadf -c`.
    let fixture = std::fs::read_to_string(fixtures().join("sadf.stream")).unwrap();
    let stream = dir.join("stream");
    std::fs::write(
        &stream,
        fixture.replacen("@@FILE 0\n", "@@FILE 0\n@@CONVERTED 0\n", 1),
    )
    .unwrap();
    runtime_with_stream(&dir, "podman", &stream);
    let stderr = dir.join("stderr");
    let mut child = sarda(&dir)
        .args(["--no-grafana", "--json"])
        .args(sa_files())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(terminate(&mut child), Some(0));

    let note = "sa20261125: old sysstat file format, converted with sadf -c";
    assert_eq!(ready["event"], "api_ready");
    assert_eq!(ready["data"]["warnings"][0], note, "{ready}");
    assert!(std::fs::read_to_string(&stderr)
        .unwrap()
        .contains(&format!("warning: {note}")));
}

#[test]
fn api_mode_serves_queries_and_stops_cleanly_on_sigterm() {
    let dir = scratch("api");
    working_podman(&dir);
    let mut child = sarda(&dir)
        .args(["--no-grafana", "--json"])
        .args(sa_files())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(ready["event"], "api_ready");
    let port: u16 = ready["api_url"]
        .as_str()
        .unwrap()
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();

    let body = http_get(
        port,
        "/api/v1/label/cpu/values?match%5B%5D=sarda_per_cpu_pct_idle",
    );
    assert!(body.contains(r#""all""#), "{body}");

    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    let stopped: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(stopped["event"], "stopped");
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

/// Real podman, real images, real Grafana. Opt in with `SARDA_E2E=1`; the
/// default sysstat and Grafana images must already be available locally.
#[test]
fn e2e_with_real_grafana() {
    if std::env::var_os("SARDA_E2E").is_none() {
        eprintln!("skipped: set SARDA_E2E=1 to run");
        return;
    }
    let path = std::env::var("PATH").unwrap();
    let mut child = Command::new(BIN)
        .env("PATH", &path)
        .args(["--json", "--port", "0"])
        .args(sa_files())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let ready: serde_json::Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    assert_eq!(ready["event"], "ready", "{ready}");
    let port = ready["grafana_port"].as_u64().unwrap() as u16;
    let container = ready["container"].as_str().unwrap().to_string();

    assert!(http_get(port, "/api/dashboards/uid/sarda").contains("\"uid\":\"sarda\""));
    assert!(http_get(port, "/api/datasources/uid/sarda/health").contains("Successfully queried"));
    assert!(http_get(
        port,
        "/api/datasources/proxy/uid/sarda/api/v1/label/host/values"
    )
    .contains("success"));

    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0));
    let ps = Command::new("podman")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&ps.stdout).contains(&container),
        "container was not removed"
    );
}
