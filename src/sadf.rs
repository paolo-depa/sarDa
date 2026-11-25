//! Running `sadf` and decoding its output.
//!
//! One container run converts every SA file for every Activity. The shell
//! script inside prints a marker-delimited stream on stdout:
//!
//! ```text
//! @@FILE 0
//! @@BEGIN per_cpu
//! <sadf -d output>
//! @@RC 0
//! @@ERR <a stderr line>
//! @@END
//! ```
//!
//! The same stream format is what the test fixtures contain.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::catalog::ACTIVITIES;
use crate::error::{Error, Result};
use crate::runtime::Runtime;

#[derive(Debug, Clone, Default)]
pub struct RawActivity {
    pub label: String,
    pub rc: i32,
    pub stdout: String,
    pub stderr: Vec<String>,
}

/// Decoded stream: `files[i]` holds every Activity output for input file `i`.
#[derive(Debug, Default)]
pub struct SadfOutput {
    pub files: Vec<Vec<RawActivity>>,
}

/// Shell script run inside the Parser container. Positional parameters are
/// the SA file paths.
pub fn build_script() -> String {
    let mut s = String::from(
        "T=$(mktemp 2>/dev/null || echo /tmp/sarda.$$)\ni=0\nfor f in \"$@\"; do\necho \"@@FILE $i\"\n",
    );
    for a in ACTIVITIES {
        s.push_str(&format!(
            "echo \"@@BEGIN {l}\"; sadf -d \"$f\" -- {o} 2>\"$T\"; echo \"@@RC $?\"; sed 's/^/@@ERR /' \"$T\"; echo \"@@END\"\n",
            l = a.label,
            o = a.sadf_opts
        ));
    }
    s.push_str("i=$((i+1))\ndone\nrm -f \"$T\"\n");
    s
}

pub fn parse_stream<R: BufRead>(reader: R) -> Result<SadfOutput> {
    let mut out = SadfOutput::default();
    let mut current: Option<RawActivity> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| Error::other(format!("reading sadf output: {e}")))?;
        if let Some(rest) = line.strip_prefix("@@FILE ") {
            let idx: usize = rest
                .trim()
                .parse()
                .map_err(|_| Error::other("corrupt sadf stream"))?;
            while out.files.len() <= idx {
                out.files.push(Vec::new());
            }
        } else if let Some(label) = line.strip_prefix("@@BEGIN ") {
            current = Some(RawActivity {
                label: label.trim().to_string(),
                ..Default::default()
            });
        } else if let Some(rc) = line.strip_prefix("@@RC ") {
            if let Some(c) = current.as_mut() {
                c.rc = rc.trim().parse().unwrap_or(-1);
            }
        } else if let Some(err) = line.strip_prefix("@@ERR ") {
            if let Some(c) = current.as_mut() {
                c.stderr.push(err.to_string());
            }
        } else if line == "@@END" {
            if let (Some(c), Some(files)) = (current.take(), out.files.last_mut()) {
                files.push(c);
            }
        } else if let Some(c) = current.as_mut() {
            c.stdout.push_str(&line);
            c.stdout.push('\n');
        }
    }
    Ok(out)
}

/// Runs the script in a throwaway container created from `image`.
pub fn run_in_container(
    rt: &Runtime,
    image: &str,
    files: &[PathBuf],
    timeout: Duration,
) -> Result<SadfOutput> {
    let name = format!("sarda-sadf-{}", std::process::id());
    let mut cmd = rt.command();
    cmd.args([
        "run",
        "--rm",
        "--name",
        &name,
        "--network=none",
        "--security-opt",
        "label=disable",
        "--entrypoint",
        "sh",
    ]);
    let mut inner = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let abs =
            std::fs::canonicalize(f).map_err(|e| Error::other(format!("{}: {e}", f.display())))?;
        cmd.arg("-v").arg(format!("{}:/in/{i}:ro", abs.display()));
        inner.push(format!("/in/{i}"));
    }
    cmd.arg(image)
        .arg("-c")
        .arg(build_script())
        .arg("sh")
        .args(&inner);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| Error::other(format!("starting {}: {e}", rt.bin)))?;
    let stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");

    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let (done, timed_out, rt, name) =
            (done.clone(), timed_out.clone(), rt.clone(), name.clone());
        std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Ordering::Relaxed) {
                if start.elapsed() > timeout {
                    timed_out.store(true, Ordering::Relaxed);
                    rt.remove_container(&name);
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
    };
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let parsed = parse_stream(BufReader::new(stdout));
    let status = child.wait();
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    let err_text = err_thread.join().unwrap_or_default();

    if timed_out.load(Ordering::Relaxed) {
        return Err(Error::other(format!(
            "sadf did not finish within {}s; raise --sadf-timeout",
            timeout.as_secs()
        )));
    }
    let output = parsed?;
    let ok = status.map(|s| s.success()).unwrap_or(false);
    if !ok || output.files.is_empty() {
        return Err(Error::other(format!(
            "the sadf container failed: {}",
            err_text.trim()
        )));
    }
    Ok(output)
}

/// Developer/test helper: runs the script with the host's `sadf` instead of a
/// container. Selected by `SARDA_SADF_HOST=1`; used to record test fixtures.
pub fn run_on_host(files: &[PathBuf]) -> Result<SadfOutput> {
    let abs: Vec<PathBuf> = files
        .iter()
        .map(|f| {
            std::fs::canonicalize(f).map_err(|e| Error::other(format!("{}: {e}", f.display())))
        })
        .collect::<Result<_>>()?;
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(build_script())
        .arg("sh")
        .args(&abs)
        .stdout(Stdio::piped())
        .spawn()?;
    let parsed = parse_stream(BufReader::new(child.stdout.take().expect("piped")));
    child.wait()?;
    parsed
}

pub fn read_stream_file(path: &Path) -> Result<SadfOutput> {
    let f =
        std::fs::File::open(path).map_err(|e| Error::other(format!("{}: {e}", path.display())))?;
    parse_stream(BufReader::new(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_mentions_every_activity() {
        let s = build_script();
        for a in ACTIVITIES {
            assert!(s.contains(&format!("@@BEGIN {}", a.label)));
            assert!(s.contains(&format!("-- {} ", a.sadf_opts)));
        }
    }

    #[test]
    fn stream_round_trip() {
        let text = "@@FILE 0\n@@BEGIN io\nheader\nrow\n@@RC 0\n@@END\n\
                    @@BEGIN queue\n@@RC 1\n@@ERR bad file\n@@END\n\
                    @@FILE 1\n@@BEGIN io\nh2\n@@RC 0\n@@END\n";
        let out = parse_stream(text.as_bytes()).unwrap();
        assert_eq!(out.files.len(), 2);
        assert_eq!(out.files[0].len(), 2);
        assert_eq!(out.files[0][0].stdout, "header\nrow\n");
        assert_eq!(out.files[0][1].rc, 1);
        assert_eq!(out.files[0][1].stderr, ["bad file"]);
        assert_eq!(out.files[1][0].stdout, "h2\n");
    }
}
