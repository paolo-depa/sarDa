//! The command line. `--help` is the reference for options and exit codes.

use std::path::PathBuf;

use clap::Parser;

use crate::runtime::RuntimeChoice;

pub const DEFAULT_GRAFANA_IMAGE: &str = "docker.io/grafana/grafana-oss:12.4.3";
/// Published for every release (`.github/workflows/publish.yaml`), so a
/// connected host needs no setup and `docker save` keeps the name for an
/// air-gapped one.
pub const DEFAULT_SADF_IMAGE: &str = concat!(
    "ghcr.io/paolo-depa/sarda-sysstat:",
    env!("CARGO_PKG_VERSION")
);
pub const DEFAULT_PORT: u16 = 3000;

const AFTER_HELP: &str = "\
EXIT CODES:
  0  success (or clean shutdown after Ctrl-C / SIGTERM)
  1  unexpected error
  2  invalid command line
  3  no working container runtime (podman or docker)
  4  a required container image is missing
  5  no usable data in the SA files (or in the --start/--end window)
  6  Grafana failed to start or cannot reach the data
  7  a network port is unavailable
  8  an SA file cannot be read by the image's sysstat (newer sysstat, or not a system activity file)

ENVIRONMENT:
  SARDA_RUNTIME, SARDA_GRAFANA_IMAGE, SARDA_SADF_IMAGE provide defaults for
  --runtime, --grafana-image and --sadf-image.

Everything runs offline: sadf runs in a container without network access and
the dashboard is served from this process. Both images must already be
available locally; see --print-containerfile.";

/// Turn sysstat SA files into a Grafana dashboard with a single command.
///
/// sarda converts the given SA files with sadf (inside a container), serves
/// the data through a Prometheus-compatible API on 127.0.0.1, starts a
/// preconfigured Grafana container and prints the URL to open. Press Ctrl-C
/// to stop and clean everything up.
#[derive(Debug, Parser)]
#[command(name = "sarda", version, after_help = AFTER_HELP)]
pub struct Cli {
    /// SA files to load (typically saYYYYMMDD from /var/log/sa).
    #[arg(value_name = "SA_FILE")]
    pub files: Vec<PathBuf>,

    /// Keep only samples at or after this time (RFC 3339; no offset means UTC).
    #[arg(long, value_name = "TIME")]
    pub start: Option<String>,

    /// Keep only samples at or before this time (RFC 3339; no offset means UTC).
    #[arg(long, value_name = "TIME")]
    pub end: Option<String>,

    /// Port for Grafana on 127.0.0.1 (default: 3000, or any free port if that is taken).
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Container runtime to use.
    #[arg(long, value_enum, default_value = "auto", env = "SARDA_RUNTIME")]
    pub runtime: RuntimeChoice,

    /// Grafana image for the grafana.
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_GRAFANA_IMAGE, env = "SARDA_GRAFANA_IMAGE")]
    pub grafana_image: String,

    /// Image providing sadf (sysstat) and a POSIX shell.
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_SADF_IMAGE, env = "SARDA_SADF_IMAGE")]
    pub sadf_image: String,

    /// Give up on sadf after this many seconds.
    #[arg(long, value_name = "SECONDS", default_value_t = 600)]
    pub sadf_timeout: u64,

    /// Do not try to pull missing images.
    #[arg(long)]
    pub no_pull: bool,

    /// Do not start Grafana; only serve the Prometheus-compatible API
    /// (or just export, together with --export-csv).
    #[arg(long)]
    pub no_grafana: bool,

    /// Also write the per-Activity CSV files into DIR.
    #[arg(long, value_name = "DIR")]
    pub export_csv: Option<PathBuf>,

    /// Print machine-readable JSON events on stdout (one object per line).
    #[arg(long)]
    pub json: bool,

    /// Print debug details on stderr.
    #[arg(short, long)]
    pub verbose: bool,

    /// Print the Containerfile for the sysstat image and exit.
    #[arg(long)]
    pub print_containerfile: bool,

    /// Print a Markdown table of the supported SA activities and exit.
    #[arg(long)]
    pub print_catalog: bool,

    /// Print the shell script run inside the sadf container and exit.
    #[arg(long, hide = true)]
    pub print_sadf_script: bool,
}

impl Cli {
    pub fn is_print_mode(&self) -> bool {
        self.print_containerfile || self.print_catalog || self.print_sadf_script
    }
}
