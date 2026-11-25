//! Hermetic tests of the shell script that runs inside the Parser container.
//! A fake `sadf` on PATH stands in for the real one, so the probe, the
//! old-format conversion and the date-based skipping are exercised without a
//! container.

use std::path::{Path, PathBuf};
use std::process::Command;

use sarda::sadf::{build_script, parse_stream};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sarda-script-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    dir
}

/// Fake sadf: `good.sa` and `old.sa` are readable (the latter only after
/// `-c`), everything else fails like a non-SA file. Headers can be made to
/// report a date via the FAKE_DATE environment variable, and `-d` prints one
/// CSV row per Activity so the stream can be checked.
fn fake_sadf(dir: &Path) {
    let script = r##"#!/bin/sh
PATH=/usr/bin:/bin
case "$1" in
  -j)
    f="$3"
    case "$f" in
      *good.sa) ;;
      *) [ "$(cat "$f" 2>/dev/null)" = converted ] || { echo "Invalid system activity file: $f" >&2; exit 3; } ;;
    esac
    printf '{\n\t"sysstat": {\n\t\t"hosts": [\n\t\t\t{\n\t\t\t\t"nodename": "testhost",\n\t\t\t\t"number-of-cpus": 4,\n\t\t\t\t"file-date": "%s"\n\t\t\t}\n\t\t]\n\t}\n}\n' "${FAKE_DATE:-2026-11-25}"
    ;;
  -c)
    case "$2" in
      *old.sa) echo converted;;
      *) echo "Invalid system activity file: $2" >&2; exit 3;;
    esac
    ;;
  -d)
    echo "# hostname;interval;timestamp;tps"
    echo "testhost;1;2026-11-25 00:00:00 UTC;1.00"
    ;;
  *) exit 3;;
esac
"##;
    let path = dir.join("bin/sadf");
    std::fs::write(&path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(
    dir: &Path,
    skip: (Option<&str>, Option<&str>),
    files: &[&str],
    date: &str,
) -> (String, bool) {
    let out = Command::new("sh")
        .arg("-c")
        .arg(build_script(skip.0, skip.1))
        .arg("sh")
        .args([skip.0.unwrap_or(""), skip.1.unwrap_or("")])
        .args(files)
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", dir.join("bin").display()),
        )
        .env("FAKE_DATE", date)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.success(),
    )
}

fn touch(dir: &Path, name: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, "x").unwrap();
    p.display().to_string()
}

#[test]
fn readable_file_runs_every_activity() {
    let dir = scratch("good");
    fake_sadf(&dir);
    let good = touch(&dir, "good.sa");
    let (stdout, ok) = run(&dir, (None, None), &[&good], "2026-11-25");
    assert!(ok);
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert!(out.probes[0].ok);
    assert!(!out.probes[0].skipped);
    assert_eq!(out.probes[0].nodename(), Some("testhost"));
    assert_eq!(out.probes[0].file_date(), Some("2026-11-25"));
    assert_eq!(out.probes[0].cpus(), Some(4));
    assert_eq!(out.files[0].len(), sarda::catalog::ACTIVITIES.len());
    assert!(stdout.contains("@@BEGIN per_cpu"));
}

#[test]
fn unreadable_file_is_skipped_without_running_activities() {
    let dir = scratch("bad");
    fake_sadf(&dir);
    let bad = touch(&dir, "junk.sa");
    let (stdout, ok) = run(&dir, (None, None), &[&bad], "2026-11-25");
    assert!(ok, "the script itself succeeds; sarda reports the bad file");
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert!(!out.probes[0].ok);
    assert!(out.probes[0].skipped);
    assert!(out.files[0].is_empty());
    assert!(
        !stdout.contains("@@BEGIN"),
        "no activity should run for an unreadable file"
    );
    assert!(out.probes[0]
        .stderr
        .join(" ")
        .contains("Invalid system activity file"));
}

#[test]
fn old_format_file_is_converted() {
    let dir = scratch("old");
    fake_sadf(&dir);
    let old = touch(&dir, "old.sa");
    let (stdout, ok) = run(&dir, (None, None), &[&old], "2026-11-25");
    assert!(ok);
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert!(out.probes[0].ok, "converted files count as readable");
    assert!(out.probes[0].converted);
    assert!(!out.probes[0].skipped);
    assert_eq!(out.files[0].len(), sarda::catalog::ACTIVITIES.len());
}

#[test]
fn files_outside_the_window_are_skipped() {
    let dir = scratch("window");
    fake_sadf(&dir);
    let good = touch(&dir, "good.sa");
    let (stdout, _) = run(&dir, (Some("2026-11-26"), None), &[&good], "2026-11-25");
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert!(out.probes[0].ok && out.probes[0].skipped);
    assert!(!stdout.contains("@@BEGIN"));

    let (stdout, _) = run(&dir, (None, Some("2026-11-24")), &[&good], "2026-11-25");
    assert!(parse_stream(stdout.as_bytes()).unwrap().probes[0].skipped);

    let (stdout, _) = run(
        &dir,
        (Some("2026-11-25"), Some("2026-11-25")),
        &[&good],
        "2026-11-25",
    );
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert!(!out.probes[0].skipped, "the boundary day is read");
    assert_eq!(out.files[0].len(), sarda::catalog::ACTIVITIES.len());
}

#[test]
fn mixed_inputs_keep_their_indexes() {
    let dir = scratch("mixed");
    fake_sadf(&dir);
    let good = touch(&dir, "good.sa");
    let old = touch(&dir, "old.sa");
    let bad = touch(&dir, "junk.sa");
    let (stdout, _) = run(&dir, (None, None), &[&good, &bad, &old], "2026-11-25");
    let out = parse_stream(stdout.as_bytes()).unwrap();
    assert_eq!(out.probes.len(), 3);
    assert!(out.probes[0].ok && !out.probes[0].skipped);
    assert!(!out.probes[1].ok && out.probes[1].skipped);
    assert!(out.probes[2].ok && out.probes[2].converted);
    assert_eq!(out.files[0].len(), sarda::catalog::ACTIVITIES.len());
    assert!(out.files[1].is_empty());
    assert_eq!(out.files[2].len(), sarda::catalog::ACTIVITIES.len());
}
