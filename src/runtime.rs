//! Container runtime detection and the few container operations sarDa needs.
//! Everything here shells out to the `podman` / `docker` CLI, which share the
//! subset of commands used.

use std::os::fd::AsFd;
use std::process::{Command, Stdio};

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

    /// Runs `<bin> <args>` with the engine's output on stderr (stdout is
    /// reserved for sarDa's result), `stdin` as given, and no timeout: a slow
    /// link is not a failure, and Ctrl-C ends it.
    fn stream(&self, args: &[&str], stdin: Stdio) -> std::io::Result<bool> {
        let to_stderr = || {
            std::io::stderr()
                .as_fd()
                .try_clone_to_owned()
                .map_or_else(|_| Stdio::null(), Stdio::from)
        };
        self.command()
            .args(args)
            .stdin(stdin)
            .stdout(to_stderr())
            .stderr(Stdio::inherit())
            .status()
            .map(|s| s.success())
    }

    /// Pulls `image`. This is sarDa's only network use at run time; only
    /// `session::ensure_image` and `images::prepare` call it.
    pub fn pull(&self, image: &str) -> bool {
        self.stream(&["pull", image], Stdio::null())
            .unwrap_or(false)
    }

    /// Builds `image` from a Containerfile given on stdin, with `context`
    /// (an empty directory) as the build context.
    pub fn build(&self, image: &str, containerfile: &str, context: &std::path::Path) -> bool {
        let Ok(mut child) = self
            .command()
            .args(["build", "-t", image, "-f", "-"])
            .arg(context)
            .stdin(Stdio::piped())
            .stdout(
                std::io::stderr()
                    .as_fd()
                    .try_clone_to_owned()
                    .map_or_else(|_| Stdio::null(), Stdio::from),
            )
            .stderr(Stdio::inherit())
            .spawn()
        else {
            return false;
        };
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(containerfile.as_bytes());
        }
        child.wait().map(|s| s.success()).unwrap_or(false)
    }

    /// Writes `images` to one archive.
    pub fn save(&self, archive: &std::path::Path, images: &[&str]) -> bool {
        let archive = archive.to_string_lossy();
        let mut args = vec!["save", "-o", archive.as_ref()];
        args.extend_from_slice(images);
        self.stream(&args, Stdio::null()).unwrap_or(false)
    }

    /// Loads an archive written by [`Runtime::save`].
    pub fn load(&self, archive: &std::path::Path) -> bool {
        let archive = archive.to_string_lossy();
        self.stream(&["load", "-i", archive.as_ref()], Stdio::null())
            .unwrap_or(false)
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
