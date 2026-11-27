//! One verdict per input file: can its data be used, and if not, why.

use super::stream::{FileProbe, SadfOutput};
use super::version::{read_sa_version, Version};

impl SadfOutput {
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

#[cfg(test)]
mod tests {
    use super::super::stream::{parse_stream, Input};
    use super::super::version::{FORMAT_MAGIC, SYSSTAT_MAGIC};
    use super::*;
    use crate::testutil::Scratch;

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
    fn sa_file(dir: &Scratch, name: &str, version: Option<[u8; 3]>) -> Input {
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
        let dir = Scratch::new("a_readable_file_is_read");
        let input = sa_file(&dir, "a", Some([12, 7, 9]));
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
        let dir = Scratch::new(
            "a_converted_file_is_read_with_a_warning_and_is_not_judged_by_its_version",
        );
        // An old-format file has no version we can compare.
        let input = sa_file(&dir, "a", None);
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
        let dir = Scratch::new("a_file_outside_the_window_is_skipped_not_rejected");
        let input = sa_file(&dir, "a", Some([12, 7, 9]));
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
        let dir = Scratch::new("an_unreadable_file_reports_each_stderr_line_once");
        let input = sa_file(&dir, "a", Some([12, 7, 9]));
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
        let dir = Scratch::new("a_file_from_a_newer_sysstat_is_incompatible");
        let input = sa_file(&dir, "a", Some([12, 9, 0]));
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
        let dir = Scratch::new("equal_or_older_files_are_compatible");
        for (tag, version) in [
            ("eq", [12, 7, 9]),
            ("older", [12, 7, 5]),
            ("old", [11, 6, 1]),
        ] {
            let input = sa_file(&dir, tag, Some(version));
            let stream = format!("{IMAGE}@@FILE 0\n@@PROBE 0 0\n{HDR}\n");
            assert_eq!(verdicts(&stream, &[input])[0].state, State::Read, "{tag}");
        }
    }

    #[test]
    fn without_a_known_image_version_nothing_is_compared() {
        let dir = Scratch::new("without_a_known_image_version_nothing_is_compared");
        let input = sa_file(&dir, "a", Some([99, 0, 0]));
        let got = &verdicts(&format!("@@FILE 0\n@@PROBE 0 0\n{HDR}\n"), &[input])[0];
        assert_eq!(got.state, State::Read);
    }

    #[test]
    fn an_input_without_a_probe_is_unreadable() {
        let dir = Scratch::new("an_input_without_a_probe_is_unreadable");
        let input = sa_file(&dir, "a", Some([12, 7, 9]));
        let got = &verdicts(IMAGE, &[input])[0];
        assert_eq!(got.problem().unwrap(), "a: sadf cannot read this file");
    }

    #[test]
    fn verdicts_follow_the_order_of_the_inputs() {
        let dir = Scratch::new("verdicts_follow_the_order_of_the_inputs");
        let inputs = [
            sa_file(&dir, "good", Some([12, 7, 9])),
            sa_file(&dir, "junk", Some([12, 7, 9])),
            sa_file(&dir, "newer", Some([13, 0, 0])),
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
}
