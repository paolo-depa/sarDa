//! Turning the paths on the command line into the SA files of a Session.
//!
//! A directory stands for the daily files in it (`saDD` or `saYYYYMMDD`, never
//! the text reports `sarDD`); the name only selects candidates there, a file's
//! day still comes from its header. A path that is not an SA file is skipped
//! with a warning, so `sarda /var/log/sa/*` works.

use std::path::{Path, PathBuf};

use crate::error::{Error, Kind, Result};
use crate::sadf::{read_sa_header, Input, SaHeader};
use crate::session::Reporter;

/// `sa` followed by two or eight digits.
fn is_daily_file_name(name: &str) -> bool {
    name.strip_prefix("sa")
        .is_some_and(|d| matches!(d.len(), 2 | 8) && d.bytes().all(|b| b.is_ascii_digit()))
}

fn daily_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_candidate = path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(is_daily_file_name);
        if is_candidate {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

pub fn resolve(paths: &[PathBuf], rep: &Reporter) -> Result<Vec<Input>> {
    if paths.is_empty() {
        return Err(Error::new(Kind::Usage, "no SA files given (see --help)"));
    }
    let mut inputs = Vec::new();
    let mut rejected = false;
    let mut consider = |path: &Path, inputs: &mut Vec<Input>| match read_sa_header(path) {
        SaHeader::NotSa => {
            rejected = true;
            rep.warn(format!(
                "{} is not a system activity file; skipping",
                path.display()
            ));
        }
        SaHeader::Current(_) | SaHeader::OldFormat => inputs.push(Input::new(path)),
    };
    for path in paths {
        if path.is_dir() {
            match daily_files(path) {
                Ok(files) if files.is_empty() => rep.warn(format!(
                    "{} holds no saDD / saYYYYMMDD files; skipping",
                    path.display()
                )),
                Ok(files) => files.iter().for_each(|f| consider(f, &mut inputs)),
                Err(e) => rep.warn(format!("{}: {e}; skipping", path.display())),
            }
        } else if path.is_file() {
            consider(path, &mut inputs);
        } else {
            rep.warn(format!("{} does not exist; skipping", path.display()));
        }
    }
    if inputs.is_empty() {
        let (kind, message) = if rejected {
            (
                Kind::IncompatibleFile,
                "none of the given files is a system activity file",
            )
        } else {
            (Kind::Usage, "none of the given paths holds an SA file")
        };
        return Err(Error::new(kind, message));
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Scratch;

    fn sa(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let mut bytes = 0xd596u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0x2175u16.to_le_bytes());
        bytes.extend_from_slice(&[12, 7, 9, 0]);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn text(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "Linux 6.4 (host)  11/25/26  _x86_64_\n").unwrap();
        path
    }

    fn names(inputs: &[Input]) -> Vec<&str> {
        inputs.iter().map(|i| i.name.as_str()).collect()
    }

    fn quiet() -> Reporter {
        Reporter {
            json: false,
            verbose: false,
        }
    }

    #[test]
    fn daily_file_names() {
        for yes in ["sa25", "sa20261125"] {
            assert!(is_daily_file_name(yes), "{yes}");
        }
        for no in ["sar25", "sar20261125", "sa2", "sa202611250", "sa.bak", "sa"] {
            assert!(!is_daily_file_name(no), "{no}");
        }
    }

    #[test]
    fn a_directory_stands_for_its_daily_files_in_order() {
        let dir = Scratch::new("inputs-dir");
        sa(&dir, "sa20261126");
        sa(&dir, "sa20261125");
        text(&dir, "sar20261125");
        let got = resolve(&[dir.to_path_buf()], &quiet()).unwrap();
        assert_eq!(names(&got), ["sa20261125", "sa20261126"]);
    }

    #[test]
    fn a_text_report_among_the_files_is_skipped() {
        let dir = Scratch::new("inputs-text");
        let good = sa(&dir, "sa20261125");
        let report = text(&dir, "sar20261125");
        let got = resolve(&[good, report], &quiet()).unwrap();
        assert_eq!(names(&got), ["sa20261125"]);
    }

    #[test]
    fn nothing_usable_is_an_incompatible_file_error_when_something_was_rejected() {
        let dir = Scratch::new("inputs-none");
        let report = text(&dir, "sar20261125");
        let err = resolve(&[report], &quiet()).unwrap_err();
        assert_eq!(err.kind, Kind::IncompatibleFile);
    }

    #[test]
    fn nothing_found_is_a_usage_error() {
        let dir = Scratch::new("inputs-missing");
        let err = resolve(&[dir.join("absent"), dir.to_path_buf()], &quiet()).unwrap_err();
        assert_eq!(err.kind, Kind::Usage);
        assert_eq!(resolve(&[], &quiet()).unwrap_err().kind, Kind::Usage);
    }
}
