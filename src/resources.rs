//! Session resources: the containers and temp paths one Session creates, and
//! the single place that knows how they are named, removed and recognised.
//!
//! Every container carries the session label (value: the owning PID) and a
//! name of the form `sarda-<pid>-<role>`, so a later run can tell which
//! leftovers belong to a process that no longer exists.
//!
//! [`Resources`] is the owner: created once per run, it removes everything it
//! was told about when dropped (success, early `?` return, panic). The
//! [`Handle`] it derefs to is what other modules receive to register what they
//! create. The signal handler holds a clone and runs the same idempotent
//! [`Handle::remove_all`].

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::runtime::Runtime;

pub const LABEL: &str = "io.github.sarda.session";
const NAME_PREFIX: &str = "sarda-";
const EXIT_INTERRUPTED: i32 = 130;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Sadf,
    Grafana,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Sadf => "sadf",
            Role::Grafana => "grafana",
        }
    }
}

/// A container name plus the `run` arguments that make it recognisable.
#[derive(Debug, Clone)]
pub struct Container {
    pub name: String,
    /// `--name <name> --label <LABEL>=<pid>`, to be passed to `run`.
    pub args: Vec<String>,
}

struct State {
    runtime: Runtime,
    pid: u32,
    temp_base: PathBuf,
    containers: Mutex<Vec<String>>,
    dirs: Mutex<Vec<PathBuf>>,
    serving: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct Handle(Arc<State>);

impl Handle {
    /// Registers a container this process is about to create.
    pub fn claim_container(&self, role: Role) -> Container {
        let name = format!("{NAME_PREFIX}{}-{}", self.0.pid, role.as_str());
        lock(&self.0.containers).push(name.clone());
        let args = vec![
            "--name".into(),
            name.clone(),
            "--label".into(),
            format!("{LABEL}={}", self.0.pid),
        ];
        Container { name, args }
    }

    /// Registers (and returns the path of) this run's temp directory.
    pub fn claim_temp_dir(&self) -> PathBuf {
        let dir = self
            .0
            .temp_base
            .join(format!("{NAME_PREFIX}{}", self.0.pid));
        lock(&self.0.dirs).push(dir.clone());
        dir
    }

    /// Forgets a container that removed itself (`run --rm`).
    pub fn release(&self, name: &str) {
        lock(&self.0.containers).retain(|n| n != name);
    }

    /// Removes one container now, keeping it registered.
    pub fn remove_container(&self, name: &str) {
        self.0.runtime.remove_container(name);
    }

    /// Removes every registered container and directory. Idempotent.
    pub fn remove_all(&self) {
        let containers = std::mem::take(&mut *lock(&self.0.containers));
        for name in containers {
            self.remove_container(&name);
        }
        let dirs = std::mem::take(&mut *lock(&self.0.dirs));
        for dir in dirs {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// From now on a signal asks the main thread to shut down gracefully
    /// instead of tearing everything down and exiting.
    pub fn mark_serving(&self) {
        self.0.serving.store(true, Ordering::SeqCst);
    }

    /// Installs the process-wide Ctrl-C / SIGTERM handler. Before
    /// [`Handle::mark_serving`] it removes everything and exits 130; after it,
    /// it notifies `shutdown`.
    pub fn handle_signals(&self, shutdown: Sender<()>) {
        let handle = self.clone();
        let _ = ctrlc::set_handler(move || {
            if handle.0.serving.load(Ordering::SeqCst) {
                let _ = shutdown.send(());
            } else {
                handle.remove_all();
                std::process::exit(EXIT_INTERRUPTED);
            }
        });
    }

    /// Removes containers left behind by an earlier run whose process is gone.
    pub fn sweep_stale(&self) {
        let rt = &self.0.runtime;
        let format = format!("{{{{.Names}}}} {{{{.Label \"{LABEL}\"}}}}");
        let Ok(out) = rt
            .command()
            .args(["ps", "-a", "--filter", &format!("label={LABEL}")])
            .args(["--format", &format])
            .stderr(Stdio::null())
            .output()
        else {
            return;
        };
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.split_whitespace();
            let (Some(name), Some(pid)) = (parts.next(), parts.next()) else {
                continue;
            };
            if let Ok(pid) = pid.parse::<u32>() {
                if pid != self.0.pid && !process_alive(pid) {
                    rt.remove_container(name);
                }
            }
        }
    }
}

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The owner. Dropping it removes everything registered, whatever the exit
/// path.
pub struct Resources(Handle);

impl Resources {
    pub fn new(runtime: Runtime) -> Resources {
        Resources::with_temp_base(runtime, std::env::temp_dir())
    }

    pub fn with_temp_base(runtime: Runtime, temp_base: PathBuf) -> Resources {
        Resources(Handle(Arc::new(State {
            runtime,
            pid: std::process::id(),
            temp_base,
            containers: Mutex::new(Vec::new()),
            dirs: Mutex::new(Vec::new()),
            serving: AtomicBool::new(false),
        })))
    }
}

impl Deref for Resources {
    type Target = Handle;
    fn deref(&self) -> &Handle {
        &self.0
    }
}

impl Drop for Resources {
    fn drop(&mut self) {
        self.0.remove_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Tests that run a freshly written script must not overlap: another
    /// thread forking while the script is still open for writing makes the
    /// exec fail with ETXTBSY, and `remove_container` ignores spawn errors.
    static EXEC_LOCK: Mutex<()> = Mutex::new(());

    /// A fake runtime that appends its arguments to `log` and answers `ps`
    /// with the lines of `ps.txt` next to it.
    struct Fake {
        dir: PathBuf,
        runtime: Runtime,
        _serial: MutexGuard<'static, ()>,
    }

    impl Fake {
        fn new(name: &str, ps_lines: &[String]) -> Fake {
            let serial = lock(&EXEC_LOCK);
            let dir =
                std::env::temp_dir().join(format!("sarda-resources-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("ps.txt"), ps_lines.join("\n")).unwrap();
            let bin = dir.join("fake-runtime");
            std::fs::write(
                &bin,
                format!(
                    "#!/bin/sh\necho \"$@\" >> '{log}'\nif [ \"$1\" = ps ]; then cat '{ps}'; fi\n",
                    log = dir.join("log").display(),
                    ps = dir.join("ps.txt").display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            Fake {
                runtime: Runtime {
                    bin: bin.display().to_string(),
                },
                dir,
                _serial: serial,
            }
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.dir.join("log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn claimed_containers_are_named_and_labelled_by_pid() {
        let fake = Fake::new("claim", &[]);
        let res = Resources::new(fake.runtime.clone());
        let pid = std::process::id();
        let c = res.claim_container(Role::Grafana);
        assert_eq!(c.name, format!("sarda-{pid}-grafana"));
        assert_eq!(
            c.args,
            ["--name", &c.name, "--label", &format!("{LABEL}={pid}")]
        );
        assert_eq!(
            res.claim_container(Role::Sadf).name,
            format!("sarda-{pid}-sadf")
        );
    }

    #[test]
    fn remove_all_removes_everything_once() {
        let fake = Fake::new("remove-all", &[]);
        let base = fake.dir.join("tmp");
        std::fs::create_dir_all(&base).unwrap();
        let res = Resources::with_temp_base(fake.runtime.clone(), base);
        let grafana = res.claim_container(Role::Grafana);
        let sadf = res.claim_container(Role::Sadf);
        let dir = res.claim_temp_dir();
        std::fs::create_dir_all(dir.join("provisioning")).unwrap();

        res.remove_all();
        res.remove_all();

        let calls = fake.calls();
        assert_eq!(calls.len(), 2, "idempotent: {calls:?}");
        assert!(calls.contains(&format!("rm -f {}", grafana.name)));
        assert!(calls.contains(&format!("rm -f {}", sadf.name)));
        assert!(!dir.exists());
    }

    #[test]
    fn released_containers_are_not_removed() {
        let fake = Fake::new("release", &[]);
        let res = Resources::new(fake.runtime.clone());
        let sadf = res.claim_container(Role::Sadf);
        let grafana = res.claim_container(Role::Grafana);
        res.release(&sadf.name);
        res.remove_all();
        assert_eq!(fake.calls(), [format!("rm -f {}", grafana.name)]);
    }

    #[test]
    fn dropping_the_owner_removes_everything() {
        let fake = Fake::new("drop", &[]);
        let name;
        {
            let res = Resources::new(fake.runtime.clone());
            name = res.claim_container(Role::Grafana).name;
        }
        assert_eq!(fake.calls(), [format!("rm -f {name}")]);
    }

    #[test]
    fn handles_share_state_with_the_owner() {
        let fake = Fake::new("handle", &[]);
        let res = Resources::new(fake.runtime.clone());
        let clone = (*res).clone();
        let name = clone.claim_container(Role::Sadf).name;
        drop(clone);
        drop(res);
        assert_eq!(fake.calls(), [format!("rm -f {name}")]);
    }

    fn dead_pid() -> u32 {
        (2_000_000..).find(|p| !process_alive(*p)).unwrap()
    }

    #[test]
    fn sweep_removes_only_containers_of_dead_processes() {
        let me = std::process::id();
        let dead = dead_pid();
        let fake = Fake::new(
            "sweep",
            &[
                format!("sarda-{dead}-grafana {dead}"),
                format!("sarda-{dead}-sadf {dead}"),
                format!("sarda-{me}-grafana {me}"),
                format!("sarda-{}-grafana {}", 1, 1),
                "unrelated-name no-pid".into(),
                "lonely-name".into(),
            ],
        );
        let res = Resources::new(fake.runtime.clone());
        res.sweep_stale();

        let calls = fake.calls();
        assert!(calls[0].starts_with("ps -a --filter label=io.github.sarda.session"));
        assert!(calls[0].contains(".Label"), "{}", calls[0]);
        let removed: Vec<&String> = calls.iter().filter(|c| c.starts_with("rm -f")).collect();
        assert_eq!(removed.len(), 2, "{calls:?}");
        assert!(removed.contains(&&format!("rm -f sarda-{dead}-grafana")));
        assert!(removed.contains(&&format!("rm -f sarda-{dead}-sadf")));
    }
}
