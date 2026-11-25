//! Running `sadf` and decoding its output.
//!
//! One container run converts every SA file for every Activity. The shell
//! script inside prints a marker-delimited stream on stdout:
//!
//! ```text
//! @@FILE 0
//! @@PROBE 0 0
//! @@HDR {"sysstat": {"hosts": [{"nodename": "h", "file-date": "2026-11-25"}]}}
//! @@PERR <a sadf stderr line>
//! @@BEGIN per_cpu
//! <sadf -d output>
//! @@RC 0
//! @@ERR <a sadf stderr line>
//! @@END
//! ```
//!
//! Per file it first probes the header with `sadf -j -H` (falling back to
//! `sadf -c` for datafiles in an old format), reports `@@CONVERTED` when the
//! fallback worked, and skips the file with `@@SKIP` when it cannot be read or
//! when its day lies outside the Observation window. The same stream format is
//! what the test fixtures contain.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

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

/// What the header probe of one input file found.
#[derive(Debug, Clone, Default)]
pub struct FileProbe {
    pub index: usize,
    /// `sadf` could read the file (directly or after `sadf -c`).
    pub ok: bool,
    /// Parsed `sadf -j -H` header, when available.
    pub header: Option<Value>,
    /// stderr of the probe and of the conversion attempt.
    pub stderr: Vec<String>,
    /// The file was in an old format and `sadf -c` converted it.
    pub converted: bool,
    /// No Activity was read from this file (unreadable or outside the window).
    pub skipped: bool,
}

impl FileProbe {
    fn host(&self) -> Option<&Value> {
        self.header.as_ref()?.get("sysstat")?.get("hosts")?.get(0)
    }

    /// The day the file covers, from its header (`YYYY-MM-DD`).
    pub fn file_date(&self) -> Option<&str> {
        self.host()?.get("file-date")?.as_str()
    }

    pub fn nodename(&self) -> Option<&str> {
        self.host()?.get("nodename")?.as_str()
    }

    pub fn cpus(&self) -> Option<u64> {
        self.host()?.get("number-of-cpus")?.as_u64()
    }
}

/// Decoded stream: `files[i]` holds every Activity output for input file `i`,
/// `probes[i]` what the header probe said about it.
#[derive(Debug, Default)]
pub struct SadfOutput {
    pub files: Vec<Vec<RawActivity>>,
    pub probes: Vec<FileProbe>,
}

impl SadfOutput {
    fn probe_mut(&mut self, index: usize) -> &mut FileProbe {
        while self.probes.len() <= index {
            let i = self.probes.len();
            self.probes.push(FileProbe {
                index: i,
                ..Default::default()
            });
        }
        &mut self.probes[index]
    }
}

/// Shell script run inside the Parser container. `$1` and `$2` are the
/// inclusive date bounds of files to skip (`YYYY-MM-DD`, empty for none), and
/// the remaining positional parameters are the SA file paths.
pub fn build_script(skip_before: Option<&str>, skip_after: Option<&str>) -> String {
    let mut s = format!(
        "T=$(mktemp 2>/dev/null || echo /tmp/sarda.$$)\n\
         H=\"$T.h\"\n\
         C=\"$T.c\"\n\
         SB=\"{}\"\n\
         SA=\"{}\"\n\
         shift 2\n\
         i=0\n\
         for f in \"$@\"; do\n\
         echo \"@@FILE $i\"\n\
         : >\"$T\"\n\
         p=1\n\
         if sadf -j -H \"$f\" >\"$H\" 2>\"$T\"; then\n\
         p=0\n\
         elif sadf -c \"$f\" >\"$C\" 2>>\"$T\" && sadf -j -H \"$C\" >\"$H\" 2>>\"$T\"; then\n\
         p=0\n\
         f=\"$C\"\n\
         echo \"@@CONVERTED $i\"\n\
         fi\n\
         echo \"@@PROBE $i $p\"\n\
         if [ -s \"$H\" ]; then echo \"@@HDR $(tr -d '\\n\\r\\t' <\"$H\")\"; fi\n\
         sed 's/^/@@PERR /' \"$T\"\n\
         d=$(sed -n 's/.*\"file-date\":[ ]*\"\\([0-9][0-9]*-[0-9][0-9]*-[0-9][0-9]*\\)\".*/\\1/p' \"$H\" | head -n 1)\n\
         skip=0\n\
         if [ \"$p\" != 0 ]; then skip=1; fi\n\
         if [ -n \"$d\" ] && [ -n \"$SB\" ] && expr \"$d\" \"<\" \"$SB\" >/dev/null 2>&1; then skip=1; fi\n\
         if [ -n \"$d\" ] && [ -n \"$SA\" ] && expr \"$d\" \">\" \"$SA\" >/dev/null 2>&1; then skip=1; fi\n\
         if [ \"$skip\" = 1 ]; then echo \"@@SKIP $i\"; i=$((i+1)); continue; fi\n",
        skip_before.unwrap_or(""),
        skip_after.unwrap_or("")
    );
    for a in ACTIVITIES {
        s.push_str(&format!(
            "echo \"@@BEGIN {l}\"; sadf -d \"$f\" -- {o} 2>\"$T\"; echo \"@@RC $?\"; sed 's/^/@@ERR /' \"$T\"; echo \"@@END\"\n",
            l = a.label,
            o = a.sadf_opts
        ));
    }
    s.push_str("i=$((i+1))\ndone\nrm -f \"$T\" \"$H\" \"$C\"\n");
    s
}

pub fn parse_stream<R: BufRead>(reader: R) -> Result<SadfOutput> {
    let mut out = SadfOutput::default();
    let mut current: Option<RawActivity> = None;
    let mut probe: Option<usize> = None;
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
            out.probe_mut(idx);
        } else if let Some(rest) = line.strip_prefix("@@PROBE ") {
            let mut parts = rest.split_whitespace();
            let idx: usize = parts
                .next()
                .and_then(|p| p.parse().ok())
                .ok_or_else(|| Error::other("corrupt sadf stream"))?;
            let rc: i32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(1);
            out.probe_mut(idx).ok = rc == 0;
            probe = Some(idx);
        } else if let Some(rest) = line.strip_prefix("@@HDR ") {
            let value: Value = serde_json::from_str(rest.trim())
                .map_err(|e| Error::other(format!("corrupt sadf header: {e}")))?;
            if let Some(i) = probe {
                out.probe_mut(i).header = Some(value);
            }
        } else if let Some(rest) = line.strip_prefix("@@PERR ") {
            if let Some(i) = probe {
                out.probe_mut(i).stderr.push(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("@@CONVERTED ") {
            if let Ok(idx) = rest.trim().parse::<usize>() {
                out.probe_mut(idx).converted = true;
            }
        } else if let Some(rest) = line.strip_prefix("@@SKIP ") {
            if let Ok(idx) = rest.trim().parse::<usize>() {
                out.probe_mut(idx).skipped = true;
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
    skip: (Option<&str>, Option<&str>),
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
        .arg(build_script(skip.0, skip.1))
        .arg("sh")
        .args([skip.0.unwrap_or(""), skip.1.unwrap_or("")])
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
pub fn run_on_host(files: &[PathBuf], skip: (Option<&str>, Option<&str>)) -> Result<SadfOutput> {
    let abs: Vec<PathBuf> = files
        .iter()
        .map(|f| {
            std::fs::canonicalize(f).map_err(|e| Error::other(format!("{}: {e}", f.display())))
        })
        .collect::<Result<_>>()?;
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(build_script(skip.0, skip.1))
        .arg("sh")
        .args([skip.0.unwrap_or(""), skip.1.unwrap_or("")])
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
        let s = build_script(None, None);
        for a in ACTIVITIES {
            assert!(s.contains(&format!("@@BEGIN {}", a.label)));
            assert!(s.contains(&format!("-- {} ", a.sadf_opts)));
        }
    }

    #[test]
    fn script_probes_skips_and_converts() {
        let s = build_script(Some("2026-01-01"), Some("2026-12-31"));
        assert!(s.contains("sadf -j -H \"$f\""));
        assert!(s.contains("sadf -c \"$f\""));
        assert!(s.contains("@@CONVERTED $i"));
        assert!(s.contains("@@SKIP $i"));
        assert!(s.contains("SB=\"2026-01-01\""));
        assert!(s.contains("SA=\"2026-12-31\""));
        assert!(s.contains("expr \"$d\" \"<\" \"$SB\""));
        let plain = build_script(None, None);
        assert!(plain.contains("SB=\"\""));
        assert!(plain.contains("SA=\"\""));
    }

    #[test]
    fn stream_round_trip() {
        let text = "@@FILE 0\n@@PROBE 0 0\n@@HDR {\"sysstat\":{\"hosts\":[{\"nodename\":\"h\",\"file-date\":\"2026-11-25\",\"number-of-cpus\":16}]}}\n\
                    @@BEGIN io\nheader\nrow\n@@RC 0\n@@END\n\
                    @@BEGIN queue\n@@RC 1\n@@ERR bad file\n@@END\n\
                    @@FILE 1\n@@PROBE 1 0\n@@CONVERTED 1\n@@SKIP 1\n\
                    @@FILE 2\n@@PROBE 2 3\n@@PERR Invalid system activity file: /in/2\n@@SKIP 2\n";
        let out = parse_stream(text.as_bytes()).unwrap();
        assert_eq!(out.files.len(), 3);
        assert_eq!(out.files[0].len(), 2);
        assert_eq!(out.files[0][0].stdout, "header\nrow\n");
        assert_eq!(out.files[0][1].rc, 1);
        assert_eq!(out.files[0][1].stderr, ["bad file"]);
        assert!(out.files[1].is_empty());

        assert!(out.probes[0].ok && !out.probes[0].skipped);
        assert_eq!(out.probes[0].file_date(), Some("2026-11-25"));
        assert_eq!(out.probes[0].nodename(), Some("h"));
        assert_eq!(out.probes[0].cpus(), Some(16));

        assert!(out.probes[1].ok && out.probes[1].converted && out.probes[1].skipped);
        assert!(out.probes[1].header.is_none());

        assert!(!out.probes[2].ok && out.probes[2].skipped);
        assert_eq!(
            out.probes[2].stderr,
            ["Invalid system activity file: /in/2"]
        );
    }

    #[test]
    fn corrupt_header_is_an_error() {
        assert!(parse_stream("@@FILE 0\n@@PROBE 0 0\n@@HDR not-json\n".as_bytes()).is_err());
    }
}
