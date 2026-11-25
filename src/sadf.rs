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
use crate::resources::{Handle, Role};
use crate::runtime::Runtime;

#[derive(Debug, Clone, Default)]
pub struct RawActivity {
    pub label: String,
    pub rc: i32,
    pub stdout: String,
    pub stderr: Vec<String>,
}

/// A sysstat version, ordered so that incompatibility can be tested with `>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl Version {
    /// Finds the first `N.N[.N]` in a string such as `sysstat version 12.7.9`.
    pub fn parse(text: &str) -> Option<Version> {
        for token in text.split_whitespace() {
            let token = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
            let mut parts = token.split('.');
            let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(major), Ok(minor)) = (major.parse(), minor.parse()) else {
                continue;
            };
            let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            return Some(Version {
                major,
                minor,
                patch,
            });
        }
        None
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

const SYSSTAT_MAGIC: u16 = 0xd596;
const FORMAT_MAGIC: u16 = 0x2175;

/// Reads the sysstat version a system activity file was written with, from
/// its magic header. Errors when the file is not an SA file, or when it is in
/// a format older than the one whose layout is known here.
pub fn read_sa_version(path: &Path) -> std::result::Result<Version, String> {
    use std::io::Read;
    let mut buf = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut buf))
        .map_err(|_| "file is too short to be a system activity file".to_string())?;
    let little = u16::from_le_bytes([buf[0], buf[1]]) == SYSSTAT_MAGIC;
    let big = u16::from_be_bytes([buf[0], buf[1]]) == SYSSTAT_MAGIC;
    if !little && !big {
        return Err("not a system activity file".into());
    }
    let short = |i: usize| {
        let pair = [buf[i], buf[i + 1]];
        if little {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        }
    };
    match short(2) {
        FORMAT_MAGIC => Ok(Version {
            major: buf[4],
            minor: buf[5],
            patch: buf[6],
        }),
        _ => Err("old system activity file format".into()),
    }
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

/// One SA file given on the command line: where it is and what to call it.
/// Everything the run learns per file is indexed by the file's position among
/// the inputs.
#[derive(Debug, Clone)]
pub struct Input {
    pub path: PathBuf,
    /// File name, used wherever the file is mentioned to the user.
    pub name: String,
}

impl Input {
    pub fn new(path: impl Into<PathBuf>) -> Input {
        let path = path.into();
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Input { path, name }
    }
}

/// Decoded stream: `files[i]` holds every Activity output for `inputs[i]`,
/// `probes[i]` what the header probe said about it.
#[derive(Debug, Default)]
pub struct SadfOutput {
    pub inputs: Vec<Input>,
    pub files: Vec<Vec<RawActivity>>,
    pub probes: Vec<FileProbe>,
    /// Version of the `sadf` that produced this stream.
    pub sadf_version: Option<Version>,
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

    pub fn with_inputs(mut self, inputs: &[Input]) -> SadfOutput {
        self.inputs = inputs.to_vec();
        self
    }

    /// Display name of input `index`.
    pub fn name(&self, index: usize) -> &str {
        self.inputs.get(index).map_or("?", |i| i.name.as_str())
    }

    /// One verdict per input, in input order.
    pub fn verdicts(&self) -> Vec<Verdict> {
        self.inputs
            .iter()
            .enumerate()
            .map(|(i, input)| {
                let probe = self.probes.iter().find(|p| p.index == i);
                let file_version = read_sa_version(&input.path);
                Verdict::settle(&input.name, probe, self.sadf_version, file_version)
            })
            .collect()
    }
}

/// Why `sadf` cannot give the data of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incompatibility {
    /// `sadf` could not read the file (nor convert it); the stderr it printed,
    /// without repeated lines.
    Unreadable(String),
    /// Written by a newer sysstat than the image's: `sadf` would silently
    /// return no data for it.
    NewerThanImage { file: Version, image: Version },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The file's data was read.
    Read,
    /// The file's day cannot overlap the Observation window; its data was
    /// not read.
    OutsideWindow,
    Incompatible(Incompatibility),
}

/// The outcome of checking one SA file before its data is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub name: String,
    pub state: State,
    /// The file was in an old format and `sadf -c` converted it.
    pub converted: bool,
    pub host: Option<String>,
    /// The day the file covers (`YYYY-MM-DD`), from its header.
    pub date: Option<String>,
    pub cpus: Option<u64>,
}

impl Verdict {
    /// `file_version` is what the file's own magic header says; it is only
    /// compared when both versions are known (old-format files report none).
    fn settle(
        name: &str,
        probe: Option<&FileProbe>,
        image: Option<Version>,
        file_version: std::result::Result<Version, String>,
    ) -> Verdict {
        let verdict = |state, converted, probe: Option<&FileProbe>| Verdict {
            name: name.to_string(),
            state,
            converted,
            host: probe.and_then(|p| p.nodename()).map(str::to_string),
            date: probe.and_then(|p| p.file_date()).map(str::to_string),
            cpus: probe.and_then(|p| p.cpus()),
        };
        let Some(probe) = probe.filter(|p| p.ok) else {
            let mut seen: Vec<&str> = Vec::new();
            for line in probe.map_or(&[][..], |p| &p.stderr[..]) {
                if !seen.contains(&line.as_str()) {
                    seen.push(line);
                }
            }
            let why = Incompatibility::Unreadable(seen.join("; "));
            return verdict(State::Incompatible(why), false, None);
        };
        if let (Some(image), Ok(file)) = (image, file_version) {
            if file > image {
                let why = Incompatibility::NewerThanImage { file, image };
                return verdict(State::Incompatible(why), probe.converted, Some(probe));
            }
        }
        let state = if probe.skipped {
            State::OutsideWindow
        } else {
            State::Read
        };
        verdict(state, probe.converted, Some(probe))
    }

    /// Why the Session cannot continue because of this file, if it cannot.
    pub fn problem(&self) -> Option<String> {
        match &self.state {
            State::Incompatible(Incompatibility::Unreadable(why)) if why.is_empty() => {
                Some(format!("{}: sadf cannot read this file", self.name))
            }
            State::Incompatible(Incompatibility::Unreadable(why)) => {
                Some(format!("{}: sadf cannot read this file: {why}", self.name))
            }
            State::Incompatible(Incompatibility::NewerThanImage { file, image }) => Some(format!(
                "{} was written by sysstat {file}, but the sysstat image provides {image} and cannot read it",
                self.name
            )),
            _ => None,
        }
    }

    /// Something worth telling the user although the file is usable.
    pub fn warning(&self) -> Option<String> {
        self.converted.then(|| {
            format!(
                "{}: old sysstat file format, converted with sadf -c",
                self.name
            )
        })
    }

    /// What was found in the file, for debug output and for explaining why
    /// no data was found.
    pub fn detail(&self) -> Option<String> {
        let date = self.date.as_deref().unwrap_or("?");
        match &self.state {
            State::Incompatible(_) => None,
            State::OutsideWindow => Some(format!(
                "{}: outside the observation window ({date}), skipped",
                self.name
            )),
            State::Read => {
                let host = self.host.as_deref().unwrap_or("?");
                let cpus = self.cpus.map(|c| format!(", {c} CPUs")).unwrap_or_default();
                Some(format!("{}: {host}, {date}{cpus}", self.name))
            }
        }
    }
}

/// Shell script run inside the Parser container. `$1` and `$2` are the
/// inclusive date bounds of files to skip (`YYYY-MM-DD`, empty for none), and
/// the remaining positional parameters are the SA file paths.
pub fn build_script(skip_before: Option<&str>, skip_after: Option<&str>) -> String {
    // Stays within POSIX sh and coreutils (sh, mktemp, tr, cat, rm) next to
    // sadf, with no sed, grep or awk, so the parser image can move to a
    // minimal base without touching the script (ADR 0007).
    let mut s = SCRIPT
        .replace("__SB__", skip_before.unwrap_or(""))
        .replace("__SA__", skip_after.unwrap_or(""));
    for a in ACTIVITIES {
        s.push_str(&format!(
            "echo \"@@BEGIN {l}\"; sadf -d \"$f\" -- {o} 2>\"$T\"; echo \"@@RC $?\"; emit \"$T\" \"@@ERR\"; echo \"@@END\"\n",
            l = a.label,
            o = a.sadf_opts
        ));
    }
    s.push_str("i=$((i+1))\ndone\nrm -f \"$T\" \"$H\" \"$C\"\n");
    s
}

/// Prologue of the script: `$1`/`$2` are the skip bounds, the rest are files.
const SCRIPT: &str = r##"T=$(mktemp 2>/dev/null || echo /tmp/sarda.$$)
H="$T.h"
C="$T.c"
SB="__SB__"
SA="__SA__"
shift 2
echo "@@VERSION $(sadf -V 2>/dev/null | head -n 1)"
i=0
emit() { while IFS= read -r l || [ -n "$l" ]; do printf '%s %s\n' "$2" "$l"; done <"$1"; }
for f in "$@"; do
echo "@@FILE $i"
: >"$T"
p=1
if sadf -j -H "$f" >"$H" 2>"$T"; then
p=0
elif sadf -c "$f" >"$C" 2>>"$T" && sadf -j -H "$C" >"$H" 2>>"$T"; then
p=0
f="$C"
echo "@@CONVERTED $i"
fi
echo "@@PROBE $i $p"
HDR=""
if [ -s "$H" ]; then HDR=$(tr -d '\n\r\t' <"$H"); echo "@@HDR $HDR"; fi
emit "$T" "@@PERR"
d=""
case "$HDR" in
*'"file-date"'*)
rest=${HDR#*'"file-date"'}
rest=${rest#:}
rest=${rest# }
rest=${rest#'"'}
d=${rest%%'"'*}
case "$d" in
????-??-??) ;;
*) d="" ;;
esac
;;
esac
skip=0
if [ "$p" != 0 ]; then skip=1; fi
if [ -n "$d" ] && [ -n "$SB" ] && expr "$d" "<" "$SB" >/dev/null 2>&1; then skip=1; fi
if [ -n "$d" ] && [ -n "$SA" ] && expr "$d" ">" "$SA" >/dev/null 2>&1; then skip=1; fi
if [ "$skip" = 1 ]; then echo "@@SKIP $i"; i=$((i+1)); continue; fi
"##;

pub fn parse_stream<R: BufRead>(reader: R) -> Result<SadfOutput> {
    let mut out = SadfOutput::default();
    let mut current: Option<RawActivity> = None;
    let mut probe: Option<usize> = None;
    for line in reader.lines() {
        let line = line.map_err(|e| Error::other(format!("reading sadf output: {e}")))?;
        if let Some(rest) = line.strip_prefix("@@VERSION ") {
            out.sadf_version = Version::parse(rest);
        } else if let Some(rest) = line.strip_prefix("@@FILE ") {
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
    inputs: &[Input],
    skip: (Option<&str>, Option<&str>),
    timeout: Duration,
    resources: &Handle,
) -> Result<SadfOutput> {
    let container = resources.claim_container(Role::Sadf);
    let name = container.name;
    let mut cmd = rt.command();
    cmd.args(["run", "--rm"]).args(&container.args);
    cmd.args([
        "--network=none",
        "--security-opt",
        "label=disable",
        "--entrypoint",
        "sh",
    ]);
    let mut inner = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let abs = std::fs::canonicalize(&input.path)
            .map_err(|e| Error::other(format!("{}: {e}", input.path.display())))?;
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
        let (done, timed_out, resources, name) = (
            done.clone(),
            timed_out.clone(),
            resources.clone(),
            name.clone(),
        );
        std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Ordering::Relaxed) {
                if start.elapsed() > timeout {
                    timed_out.store(true, Ordering::Relaxed);
                    resources.remove_container(&name);
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
    // `run --rm` removed the container itself.
    resources.release(&name);

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
    Ok(output.with_inputs(inputs))
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
        assert!(s.contains("@@VERSION $(sadf -V"));
        assert!(s.contains("@@SKIP $i"));
        assert!(s.contains("SB=\"2026-01-01\""));
        assert!(s.contains("SA=\"2026-12-31\""));
        assert!(s.contains("expr \"$d\" \"<\" \"$SB\""));
        let plain = build_script(None, None);
        assert!(plain.contains("SB=\"\""));
        assert!(plain.contains("SA=\"\""));
        for legacy in ["sed", "grep", "awk"] {
            assert!(
                !s.contains(legacy) && !plain.contains(legacy),
                "the script must stay within POSIX sh and coreutils (no {legacy}), ADR 0007"
            );
        }
        for legacy in ["sed", "grep", "awk"] {
            assert!(
                !s.contains(legacy) && !plain.contains(legacy),
                "the script must stay within POSIX sh and coreutils (no {legacy}), ADR 0007"
            );
        }
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
    fn version_parsing_and_ordering() {
        let v = Version::parse("sysstat version 12.7.9").expect("version");
        assert_eq!((v.major, v.minor, v.patch), (12, 7, 9));
        assert_eq!(
            Version::parse("12.7").unwrap(),
            Version {
                major: 12,
                minor: 7,
                patch: 0
            }
        );
        assert!(Version::parse("(C) Sebastien Godard").is_none());
        assert!(
            v < Version {
                major: 12,
                minor: 8,
                patch: 0
            }
        );
        assert!(
            v > Version {
                major: 12,
                minor: 7,
                patch: 5
            }
        );
        assert!(
            v > Version {
                major: 11,
                minor: 9,
                patch: 9
            }
        );
    }

    #[test]
    fn reads_the_version_from_the_file_header() {
        let dir = std::env::temp_dir().join(format!("sarda-hdr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let header = |magic: fn(u16) -> [u8; 2], version: [u8; 3]| {
            let mut b = Vec::new();
            b.extend_from_slice(&magic(0xd596));
            b.extend_from_slice(&magic(0x2175));
            b.extend_from_slice(&version);
            b.extend_from_slice(&[0, 0, 0, 0]);
            b
        };
        let le = dir.join("le");
        std::fs::write(&le, header(u16::to_le_bytes, [12, 8, 1])).unwrap();
        assert_eq!(
            read_sa_version(&le).unwrap(),
            Version {
                major: 12,
                minor: 8,
                patch: 1
            }
        );

        let be = dir.join("be");
        std::fs::write(&be, header(u16::to_be_bytes, [12, 7, 9])).unwrap();
        assert_eq!(
            read_sa_version(&be).unwrap(),
            Version {
                major: 12,
                minor: 7,
                patch: 9
            }
        );

        let junk = dir.join("junk");
        std::fs::write(&junk, b"not an sa file at all").unwrap();
        assert!(read_sa_version(&junk).is_err());
        let short = dir.join("short");
        std::fs::write(&short, b"abc").unwrap();
        assert!(read_sa_version(&short).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- verdicts -------------------------------------------------------

    const HDR: &str = r#"@@HDR {"sysstat":{"hosts":[{"nodename":"h","file-date":"2026-11-25","number-of-cpus":16}]}}"#;
    const IMAGE: &str = "@@VERSION sysstat version 12.7.9\n";

    fn v(major: u8, minor: u8, patch: u8) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// An SA file whose magic header claims `version` (`None`: old format).
    fn sa_file(tag: &str, name: &str, version: Option<[u8; 3]>) -> Input {
        let dir = std::env::temp_dir().join(format!("sarda-verdict-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SYSSTAT_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&version.map_or(0x2171, |_| FORMAT_MAGIC).to_le_bytes());
        bytes.extend_from_slice(&version.unwrap_or([0, 0, 0]));
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        Input::new(path)
    }

    fn verdicts(stream: &str, inputs: &[Input]) -> Vec<Verdict> {
        parse_stream(stream.as_bytes())
            .unwrap()
            .with_inputs(inputs)
            .verdicts()
    }

    #[test]
    fn a_readable_file_is_read() {
        let input = sa_file("read", "a", Some([12, 7, 9]));
        let got = &verdicts(&format!("{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n"), &[input])[0];
        assert_eq!(got.state, State::Read);
        assert_eq!(got.host.as_deref(), Some("h"));
        assert_eq!(got.date.as_deref(), Some("2026-11-25"));
        assert_eq!(got.cpus, Some(16));
        assert_eq!(got.detail().unwrap(), "a: h, 2026-11-25, 16 CPUs");
        assert!(got.problem().is_none() && got.warning().is_none());
    }

    #[test]
    fn a_converted_file_is_read_with_a_warning_and_is_not_judged_by_its_version() {
        // An old-format file has no version we can compare.
        let input = sa_file("conv", "a", None);
        let stream = format!("{IMAGE}@@FILE 0\n@@CONVERTED 0\n@@PROBE 0 0\n{HDR}\n");
        let got = &verdicts(&stream, &[input])[0];
        assert_eq!(got.state, State::Read);
        assert!(got.converted);
        assert_eq!(
            got.warning().unwrap(),
            "a: old sysstat file format, converted with sadf -c"
        );
        assert!(got.problem().is_none());
    }

    #[test]
    fn a_file_outside_the_window_is_skipped_not_rejected() {
        let input = sa_file("outside", "a", Some([12, 7, 9]));
        let stream = format!("{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n@@SKIP 0\n");
        let got = &verdicts(&stream, &[input])[0];
        assert_eq!(got.state, State::OutsideWindow);
        assert_eq!(
            got.detail().unwrap(),
            "a: outside the observation window (2026-11-25), skipped"
        );
        assert!(got.problem().is_none());
    }

    #[test]
    fn an_unreadable_file_reports_each_stderr_line_once() {
        let input = sa_file("unreadable", "a", Some([12, 7, 9]));
        let stream = format!(
            "{IMAGE}@@FILE 0\n@@PROBE 0 3\n@@PERR Invalid system activity file: /in/0\n\
             @@PERR Invalid system activity file: /in/0\n@@PERR second line\n@@SKIP 0\n"
        );
        let got = &verdicts(&stream, &[input])[0];
        let why = "Invalid system activity file: /in/0; second line";
        assert_eq!(
            got.state,
            State::Incompatible(Incompatibility::Unreadable(why.into()))
        );
        assert_eq!(
            got.problem().unwrap(),
            format!("a: sadf cannot read this file: {why}")
        );
        assert!(got.detail().is_none());
    }

    #[test]
    fn a_file_from_a_newer_sysstat_is_incompatible() {
        let input = sa_file("newer", "a", Some([12, 9, 0]));
        let got = &verdicts(&format!("{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n"), &[input])[0];
        assert_eq!(
            got.state,
            State::Incompatible(Incompatibility::NewerThanImage {
                file: v(12, 9, 0),
                image: v(12, 7, 9)
            })
        );
        assert_eq!(
            got.problem().unwrap(),
            "a was written by sysstat 12.9.0, but the sysstat image provides 12.7.9 and cannot read it"
        );
        assert_eq!(got.host.as_deref(), Some("h"), "what was found is kept");
    }

    #[test]
    fn equal_or_older_files_are_compatible() {
        for (tag, version) in [
            ("eq", [12, 7, 9]),
            ("older", [12, 7, 5]),
            ("old", [11, 6, 1]),
        ] {
            let input = sa_file(tag, "a", Some(version));
            let stream = format!("{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n");
            assert_eq!(verdicts(&stream, &[input])[0].state, State::Read, "{tag}");
        }
    }

    #[test]
    fn without_a_known_image_version_nothing_is_compared() {
        let input = sa_file("noimage", "a", Some([99, 0, 0]));
        let got = &verdicts(&format!("@@FILE 0\n@@PROBE 0 0\n{HDR}\n"), &[input])[0];
        assert_eq!(got.state, State::Read);
    }

    #[test]
    fn an_input_without_a_probe_is_unreadable() {
        let input = sa_file("noprobe", "a", Some([12, 7, 9]));
        let got = &verdicts(IMAGE, &[input])[0];
        assert_eq!(got.problem().unwrap(), "a: sadf cannot read this file");
    }

    #[test]
    fn verdicts_follow_the_order_of_the_inputs() {
        let inputs = [
            sa_file("mixed", "good", Some([12, 7, 9])),
            sa_file("mixed", "junk", Some([12, 7, 9])),
            sa_file("mixed", "newer", Some([13, 0, 0])),
        ];
        let stream = format!(
            "{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n@@FILE 1\n@@PROBE 1 3\n@@PERR bad\n@@SKIP 1\n\
             @@FILE 2\n@@PROBE 2 0\n{HDR}\n"
        );
        let got = verdicts(&stream, &inputs);
        let names: Vec<&str> = got.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["good", "junk", "newer"]);
        assert_eq!(got[0].state, State::Read);
        assert!(matches!(
            got[1].state,
            State::Incompatible(Incompatibility::Unreadable(_))
        ));
        assert!(matches!(
            got[2].state,
            State::Incompatible(Incompatibility::NewerThanImage { .. })
        ));
    }

    #[test]
    fn an_input_is_named_after_its_file() {
        assert_eq!(Input::new("/var/log/sa/sa20261125").name, "sa20261125");
        assert_eq!(Input::new("relative/sa01").name, "sa01");
        assert_eq!(Input::new("/").name, "/");
    }

    #[test]
    fn corrupt_header_is_an_error() {
        assert!(parse_stream("@@FILE 0\n@@PROBE 0 0\n@@HDR not-json\n".as_bytes()).is_err());
    }
}
