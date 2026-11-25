//! Container runtime detection and the few container operations sarDa needs.
//! Everything here shells out to the `podman` / `docker` CLI, which share the
//! subset of commands used.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{Error, Kind, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum RuntimeChoice {
    Auto,
    Podman,
    Docker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    pub bin: String,
}

fn usable(bin: &str) -> bool {
    Command::new(bin)
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl Runtime {
    /// Finds a working runtime. `auto` prefers podman, then docker; a runtime
    /// is only accepted if `<bin> info` succeeds (so a stopped docker daemon
    /// is skipped).
    pub fn detect(choice: RuntimeChoice) -> Result<Runtime> {
        let candidates: &[&str] = match choice {
            RuntimeChoice::Auto => &["podman", "docker"],
            RuntimeChoice::Podman => &["podman"],
            RuntimeChoice::Docker => &["docker"],
        };
        for bin in candidates {
            if usable(bin) {
                return Ok(Runtime {
                    bin: (*bin).to_string(),
                });
            }
        }
        Err(Error::new(
            Kind::NoRuntime,
            format!(
                "no working container runtime found (tried: {}). Install podman or docker, make sure it can run containers, or select one with --runtime.",
                candidates.join(", ")
            ),
        ))
    }

    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.bin);
        c.stdin(Stdio::null());
        c
    }

    pub fn image_exists(&self, image: &str) -> bool {
        self.command()
            .args(["image", "inspect", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Tries to pull `image`, giving up after `timeout` (an air-gapped host
    /// may otherwise hang on name resolution).
    pub fn pull(&self, image: &str, timeout: Duration) -> bool {
        let Ok(mut child) = self
            .command()
            .args(["pull", "--quiet", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(200))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }

    pub fn remove_container(&self, name: &str) {
        let _ = self
            .command()
            .args(["rm", "-f", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    pub fn container_running(&self, name: &str) -> bool {
        self.command()
            .args([
                "container",
                "inspect",
                "--format",
                "{{.State.Running}}",
                name,
            ])
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false)
    }
}

pub fn missing_image_help(image: &str, role: &str, build_hint: &str) -> String {
    format!(
        "{role} image '{image}' is not available locally and could not be pulled.\n{build_hint}\nOverride the image name with the matching option or environment variable if yours is called differently."
    )
}
