//! The Viewer: a preconfigured Grafana container that reads the Sarda API.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::dashboard::{DASHBOARD_UID, DATASOURCE_UID};
use crate::error::{Error, Kind, Result};
use crate::runtime::Runtime;

pub const LABEL: &str = "io.github.sarda.session";
pub const NAME_PREFIX: &str = "sarda-";
const PROVISIONING_DIR: &str = "/etc/grafana/provisioning";
const DASHBOARD_FILE: &str = "sarda.json";

pub struct Viewer {
    rt: Runtime,
    name: String,
    dir: PathBuf,
    pub port: u16,
}

pub struct ViewerSpec<'a> {
    pub image: &'a str,
    pub port: u16,
    pub api_addr: SocketAddr,
    pub scrape_interval: String,
    pub dashboard: &'a Value,
}

/// Finds a free loopback port, preferring `preferred` (0 means any). With
/// `strict`, an occupied preferred port is an error instead of a fallback.
pub fn pick_port(preferred: Option<u16>, strict: bool) -> Result<u16> {
    let free = |p: u16| TcpListener::bind(("127.0.0.1", p)).is_ok();
    if let Some(p) = preferred.filter(|p| *p != 0) {
        if free(p) {
            return Ok(p);
        }
        if strict {
            return Err(Error::new(
                Kind::PortUnavailable,
                format!("port {p} is already in use on 127.0.0.1; choose another with --port or omit it to pick one automatically"),
            ));
        }
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| {
            Error::new(
                Kind::PortUnavailable,
                format!("no free port available: {e}"),
            )
        })
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        set_mode(parent, 0o755);
    }
    std::fs::write(path, content)?;
    set_mode(path, 0o644);
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) {}

pub fn datasource_yaml(api_addr: SocketAddr, scrape_interval: &str) -> String {
    format!(
        "apiVersion: 1\ndatasources:\n  - name: SAR\n    uid: {DATASOURCE_UID}\n    type: prometheus\n    access: proxy\n    url: http://{api_addr}\n    isDefault: true\n    editable: false\n    jsonData:\n      httpMethod: GET\n      timeInterval: \"{scrape_interval}\"\n"
    )
}

pub fn dashboard_provider_yaml() -> String {
    format!(
        "apiVersion: 1\nproviders:\n  - name: sarda\n    type: file\n    disableDeletion: true\n    allowUiUpdates: true\n    options:\n      path: {PROVISIONING_DIR}/dashboards/json\n"
    )
}

/// Minimal blocking HTTP GET against loopback; returns status and body.
pub fn http_get(port: u16, path: &str, timeout: Duration) -> Option<(u16, String)> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\n\r\n"
    )
    .ok()?;
    let mut text = String::new();
    stream.read_to_string(&mut text).ok()?;
    let status = text
        .strip_prefix("HTTP/1.")?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    let body = text
        .split_once("\r\n\r\n")
        .map_or(String::new(), |(_, b)| b.to_string());
    Some((status, body))
}

impl Viewer {
    pub fn start(rt: &Runtime, spec: &ViewerSpec) -> Result<Viewer> {
        let pid = std::process::id();
        let name = format!("{NAME_PREFIX}{pid}");
        let dir = std::env::temp_dir().join(format!("sarda-{pid}"));
        let prov = dir.join("provisioning");
        write_file(
            &prov.join("datasources/sarda.yaml"),
            &datasource_yaml(spec.api_addr, &spec.scrape_interval),
        )?;
        write_file(
            &prov.join("dashboards/sarda.yaml"),
            &dashboard_provider_yaml(),
        )?;
        write_file(
            &prov.join("dashboards/json").join(DASHBOARD_FILE),
            &serde_json::to_string_pretty(spec.dashboard).unwrap_or_default(),
        )?;

        let mut viewer = Viewer {
            rt: rt.clone(),
            name: name.clone(),
            dir,
            port: spec.port,
        };

        let home = format!("{PROVISIONING_DIR}/dashboards/json/{DASHBOARD_FILE}");
        let env = [
            ("GF_SERVER_HTTP_ADDR", "127.0.0.1".to_string()),
            ("GF_SERVER_HTTP_PORT", spec.port.to_string()),
            ("GF_AUTH_ANONYMOUS_ENABLED", "true".into()),
            ("GF_AUTH_ANONYMOUS_ORG_ROLE", "Admin".into()),
            ("GF_AUTH_DISABLE_LOGIN_FORM", "true".into()),
            ("GF_ANALYTICS_REPORTING_ENABLED", "false".into()),
            ("GF_ANALYTICS_CHECK_FOR_UPDATES", "false".into()),
            ("GF_ANALYTICS_CHECK_FOR_PLUGIN_UPDATES", "false".into()),
            ("GF_PLUGINS_PREINSTALL_DISABLED", "true".into()),
            ("GF_NEWS_NEWS_FEED_ENABLED", "false".into()),
            ("GF_DASHBOARDS_DEFAULT_HOME_DASHBOARD_PATH", home),
        ];
        let mut cmd = rt.command();
        cmd.args(["run", "-d", "--rm", "--name", &name, "--label"])
            .arg(format!("{LABEL}={pid}"))
            .arg("--network=host")
            .arg("-v")
            .arg(format!("{}:{PROVISIONING_DIR}:ro,z", prov.display()));
        for (k, v) in &env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        cmd.arg(spec.image);
        let out = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::new(Kind::ViewerFailed, format!("starting {}: {e}", rt.bin)))?;
        if !out.status.success() {
            viewer.cleanup();
            return Err(Error::new(
                Kind::ViewerFailed,
                format!(
                    "could not start the Grafana container: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        Ok(viewer)
    }

    fn logs(&self) -> String {
        self.rt
            .command()
            .args(["logs", "--tail", "20", &self.name])
            .output()
            .map(|o| {
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                )
            })
            .unwrap_or_default()
    }

    pub fn is_running(&self) -> bool {
        self.rt.container_running(&self.name)
    }

    /// Waits until Grafana is up, the dashboard is provisioned and the
    /// datasource can reach the Sarda API.
    pub fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        let probe = Duration::from_secs(2);
        let mut dashboard_ready = false;
        while start.elapsed() < timeout {
            if !self.is_running() {
                return Err(Error::new(
                    Kind::ViewerFailed,
                    format!(
                        "the Grafana container exited during start-up:\n{}",
                        self.logs().trim_end()
                    ),
                ));
            }
            let health = http_get(self.port, "/api/health", probe);
            if health.is_some_and(|(s, _)| s == 200) {
                if !dashboard_ready {
                    dashboard_ready = http_get(
                        self.port,
                        &format!("/api/dashboards/uid/{DASHBOARD_UID}"),
                        probe,
                    )
                    .is_some_and(|(s, _)| s == 200);
                }
                if dashboard_ready {
                    return match http_get(self.port, &format!("/api/datasources/uid/{DATASOURCE_UID}/health"), Duration::from_secs(10)) {
                        Some((200, _)) => Ok(()),
                        other => Err(Error::new(
                            Kind::ViewerFailed,
                            format!(
                                "Grafana started but its datasource cannot reach the Sarda API (health check: {:?}). Is a firewall or proxy blocking loopback connections?",
                                other.map(|(s, b)| format!("{s} {}", b.trim()))
                            ),
                        )),
                    };
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(Error::new(
            Kind::ViewerFailed,
            format!(
                "Grafana did not become ready within {}s:\n{}",
                timeout.as_secs(),
                self.logs().trim_end()
            ),
        ))
    }

    pub fn container_name(&self) -> &str {
        &self.name
    }

    fn cleanup(&mut self) {
        self.rt.remove_container(&self.name);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Drop for Viewer {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datasource_points_at_the_api() {
        let y = datasource_yaml("127.0.0.1:9999".parse().unwrap(), "1m");
        assert!(y.contains("url: http://127.0.0.1:9999"));
        assert!(y.contains("type: prometheus"));
        assert!(y.contains("uid: sarda"));
        assert!(y.contains("timeInterval: \"1m\""));
    }

    #[test]
    fn strict_port_conflict_is_an_error() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        assert_eq!(
            pick_port(Some(port), true).unwrap_err().kind,
            Kind::PortUnavailable
        );
        let fallback = pick_port(Some(port), false).unwrap();
        assert_ne!(fallback, port);
    }

    #[test]
    fn http_get_parses_status_and_body() {
        let api = crate::api::serve(
            std::sync::Arc::new(crate::store::Store::default()),
            "127.0.0.1:0",
        )
        .unwrap();
        let (status, body) =
            http_get(api.addr.port(), "/-/healthy", Duration::from_secs(2)).unwrap();
        assert_eq!(status, 200);
        assert!(body.contains("ready"));
    }
}
