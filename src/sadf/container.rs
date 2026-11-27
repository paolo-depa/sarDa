//! Running the script in a throwaway container and decoding its stdout.

use std::io::{BufReader, Read};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::resources::{Handle, Role};
use crate::runtime::Runtime;

use super::script::build_script;
use super::stream::{parse_stream_with, Input, SadfOutput};

/// Runs the script in a throwaway container created from `image`.
/// `on_file` is told the index of each input as the script starts on it. The
/// container is removed when it prints nothing for `stall`.
pub fn run_in_container(
    rt: &Runtime,
    image: &str,
    inputs: &[Input],
    skip: (Option<&str>, Option<&str>),
    stall: Duration,
    resources: &Handle,
    mut on_file: impl FnMut(usize),
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
    let started = Instant::now();
    let last_output = Arc::new(AtomicU64::new(0));
    let watchdog = {
        let (done, timed_out, resources, name, last_output) = (
            done.clone(),
            timed_out.clone(),
            resources.clone(),
            name.clone(),
            last_output.clone(),
        );
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let quiet_for =
                    started.elapsed().as_millis() as u64 - last_output.load(Ordering::Relaxed);
                if quiet_for > stall.as_millis() as u64 {
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

    let parsed = parse_stream_with(BufReader::new(stdout), |line| {
        last_output.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        if let Some(index) = line
            .strip_prefix("@@FILE ")
            .and_then(|i| i.trim().parse().ok())
        {
            on_file(index);
        }
    });
    let status = child.wait();
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    let err_text = err_thread.join().unwrap_or_default();
    // `run --rm` removed the container itself.
    resources.release(&name);

    if timed_out.load(Ordering::Relaxed) {
        return Err(Error::other(format!(
            "sadf printed nothing for {}s and was stopped; raise --sadf-timeout",
            stall.as_secs()
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
