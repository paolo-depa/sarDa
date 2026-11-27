//! The decoded marker stream: what the script reported about each input file.

use std::io::BufRead;
use std::path::PathBuf;

use serde_json::Value;

use super::version::Version;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct RawActivity {
    pub label: String,
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
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_round_trip() {
        let text = "@@FILE 0\n@@PROBE 0 0\n@@HDR {\"sysstat\":{\"hosts\":[{\"nodename\":\"h\",\"file-date\":\"2026-11-25\",\"number-of-cpus\":16}]}}\n\
                    @@BEGIN io\nheader\nrow\n@@END\n\
                    @@BEGIN queue\n@@ERR bad file\n@@END\n\
                    @@FILE 1\n@@PROBE 1 0\n@@CONVERTED 1\n@@SKIP 1\n\
                    @@FILE 2\n@@PROBE 2 3\n@@PERR Invalid system activity file: /in/2\n@@SKIP 2\n";
        let out = parse_stream(text.as_bytes()).unwrap();
        assert_eq!(out.files.len(), 3);
        assert_eq!(out.files[0].len(), 2);
        assert_eq!(out.files[0][0].stdout, "header\nrow\n");
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
