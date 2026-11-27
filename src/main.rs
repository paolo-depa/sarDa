//! Entry point: `containerfile` and `catalog` write their text to stdout and
//! exit; every other command goes through [`sarda::session`] or
//! [`sarda::images`].

use std::process::ExitCode;

use clap::Parser;
use serde_json::json;

use sarda::catalog;
use sarda::cli::{Cli, Command};
use sarda::error::{Error, Kind, Result};
use sarda::images;
use sarda::sadf;
use sarda::session::{self, Reporter};

fn dispatch(cli: &Cli, rep: &Reporter) -> Result<()> {
    if cli.stray_options() {
        return Err(Error::new(
            Kind::Usage,
            "options and files written before a subcommand are not used by it; put them after the subcommand name",
        ));
    }
    match &cli.command {
        None => session::serve_grafana(&cli.open, rep),
        Some(Command::ExportCsv(args)) => session::export_csv_only(args, rep),
        Some(Command::Api(args)) => session::serve_api_only(args, rep),
        Some(Command::Prepare(args)) => images::prepare(args, rep),
        Some(Command::Load(args)) => images::load(args, rep),
        Some(Command::Containerfile) => {
            print!("{}", session::containerfile());
            Ok(())
        }
        Some(Command::Catalog) => {
            print!("{}", catalog::markdown_table());
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    // A development aid for regenerating tests/fixtures, not a command.
    if std::env::var_os("SARDA_PRINT_SADF_SCRIPT").is_some() {
        print!("{}", sadf::build_script(None, None));
        return ExitCode::SUCCESS;
    }

    let cli = Cli::parse();
    let rep = Reporter {
        json: cli.global.json,
        verbose: cli.global.verbose,
    };
    match dispatch(&cli, &rep) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e.message);
            rep.event(json!({
                "event": "error",
                "kind": e.kind.name(),
                "exit_code": e.kind.exit_code(),
                "message": e.message,
            }));
            ExitCode::from(e.kind.exit_code() as u8)
        }
    }
}
