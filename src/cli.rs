//! The command line. `--help` is the reference for options and exit codes.
//!
//! `sarda FILES...` opens a dashboard: the arguments of that default command
//! are read at the top level, next to the other subcommands. There is no
//! `open` subcommand on purpose: options written before it would be silently
//! dropped.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::runtime::RuntimeChoice;

pub const DEFAULT_GRAFANA_IMAGE: &str = "docker.io/grafana/grafana-oss:12.4.3";
/// Published for every release (`.github/workflows/publish.yaml`), so a
/// connected host needs no setup and `sarda prepare --save` keeps the name for
/// an air-gapped one.
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
  8  an SA file cannot be read by the image's sysstat (newer sysstat, or corrupt), or no given file is a system activity file

ENVIRONMENT:
  SARDA_RUNTIME, SARDA_GRAFANA_IMAGE, SARDA_SADF_IMAGE provide defaults for
  --runtime, --grafana-image and --sadf-image.

Once the images are present everything runs offline: sadf runs in a container
without network access and the dashboard is served from this process. Images
are pulled only when missing (--no-pull forbids that); on an air-gapped host
use `sarda prepare --save` and `sarda load`.";

/// Turn sysstat SA files into a Grafana dashboard with a single command.
///
/// sarda converts the given SA files with sadf (inside a container), serves
/// the data through a Prometheus-compatible API on 127.0.0.1, starts a
/// preconfigured Grafana container and prints the URL to open. Press Ctrl-C
/// to stop and clean everything up.
#[derive(Debug, Parser)]
#[command(
    name = "sarda",
    version,
    after_help = AFTER_HELP,
    subcommand_precedence_over_arg = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// With no subcommand: convert the files, start Grafana and print the
    /// dashboard URL.
    #[command(flatten)]
    pub open: OpenArgs,
}

#[derive(Debug, Args)]
pub struct Global {
    /// Print machine-readable JSON events on stdout (one object per line).
    #[arg(long, global = true)]
    pub json: bool,

    /// Print debug details on stderr.
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Convert SA files and write the per-Activity CSV files, then exit.
    ExportCsv(ExportCsvArgs),
    /// Convert SA files and serve only the Prometheus-compatible API.
    Api(ApiArgs),
    /// Pull the images sarda needs, and optionally save them to one archive
    /// to carry to an air-gapped host.
    Prepare(PrepareArgs),
    /// Load an archive written by `sarda prepare --save`.
    Load(LoadArgs),
    /// Print the Containerfile of the sysstat image and exit.
    Containerfile,
    /// Print a Markdown table of the supported SA activities and exit.
    Catalog,
}

/// What every command that reads SA files takes.
#[derive(Debug, PartialEq, Eq, Args)]
pub struct InputArgs {
    /// SA files to load, or directories holding them (typically
    /// /var/log/sa). Files that are not SA files are skipped with a warning.
    #[arg(value_name = "SA_FILE|DIR")]
    pub files: Vec<PathBuf>,

    /// Keep only samples at or after this time (RFC 3339; no offset means UTC).
    #[arg(long, value_name = "TIME")]
    pub start: Option<String>,

    /// Keep only samples at or before this time (RFC 3339; no offset means UTC).
    #[arg(long, value_name = "TIME")]
    pub end: Option<String>,

    /// Give up on sadf when it prints nothing for this many seconds.
    #[arg(long, value_name = "SECONDS", default_value_t = 120)]
    pub sadf_timeout: u64,
}

/// The container runtime and the sysstat image.
#[derive(Debug, PartialEq, Eq, Args)]
pub struct EngineArgs {
    /// Container runtime to use.
    #[arg(long, value_enum, default_value = "auto", env = "SARDA_RUNTIME")]
    pub runtime: RuntimeChoice,

    /// Image providing sadf (sysstat) and a POSIX shell.
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_SADF_IMAGE, env = "SARDA_SADF_IMAGE")]
    pub sadf_image: String,

    /// Do not try to pull missing images.
    #[arg(long)]
    pub no_pull: bool,
}

#[derive(Debug, PartialEq, Eq, Args)]
pub struct GrafanaArgs {
    /// Grafana image.
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_GRAFANA_IMAGE, env = "SARDA_GRAFANA_IMAGE")]
    pub grafana_image: String,

    /// Port for Grafana on 127.0.0.1 (default: 3000, or any free port if that is taken).
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Open the dashboard in the default browser (xdg-open).
    #[arg(long)]
    pub open: bool,
}

#[derive(Debug, PartialEq, Eq, Args)]
pub struct OpenArgs {
    #[command(flatten)]
    pub input: InputArgs,
    #[command(flatten)]
    pub engine: EngineArgs,
    #[command(flatten)]
    pub grafana: GrafanaArgs,
}

#[derive(Debug, Args)]
pub struct ExportCsvArgs {
    /// Directory to write the CSV files into (created if missing).
    #[arg(value_name = "DIR")]
    pub dir: PathBuf,
    #[command(flatten)]
    pub input: InputArgs,
    #[command(flatten)]
    pub engine: EngineArgs,
}

#[derive(Debug, Args)]
pub struct ApiArgs {
    #[command(flatten)]
    pub input: InputArgs,
    #[command(flatten)]
    pub engine: EngineArgs,
}

#[derive(Debug, Args)]
pub struct PrepareArgs {
    /// Container runtime to use.
    #[arg(long, value_enum, default_value = "auto", env = "SARDA_RUNTIME")]
    pub runtime: RuntimeChoice,

    /// Sysstat image to pull (or to build with --build).
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_SADF_IMAGE, env = "SARDA_SADF_IMAGE")]
    pub sadf_image: String,

    /// Grafana image to pull.
    #[arg(long, value_name = "IMAGE", default_value = DEFAULT_GRAFANA_IMAGE, env = "SARDA_GRAFANA_IMAGE")]
    pub grafana_image: String,

    /// Build the sysstat image from the Containerfile instead of pulling it.
    #[arg(long)]
    pub build: bool,

    /// Write both images to this archive (for `sarda load` on another host).
    #[arg(long, value_name = "FILE")]
    pub save: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct LoadArgs {
    /// Archive written by `sarda prepare --save`.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,

    /// Container runtime to use.
    #[arg(long, value_enum, default_value = "auto", env = "SARDA_RUNTIME")]
    pub runtime: RuntimeChoice,
}

impl Cli {
    /// The default command's options, when a subcommand was given as well:
    /// they would be dropped silently, so the caller must refuse them.
    pub fn stray_options(&self) -> bool {
        self.command.is_some() && self.open != Cli::parse_from(["sarda"]).open
    }
}
