use std::process::ExitCode;

use clap::Parser;
use serde_json::json;

use sarda::catalog;
use sarda::cli::Cli;
use sarda::sadf;
use sarda::session::{self, Reporter};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let rep = Reporter {
        json: cli.json,
        verbose: cli.verbose,
    };

    if cli.is_print_mode() {
        if cli.print_containerfile {
            print!("{}", session::containerfile());
        } else if cli.print_catalog {
            print!("{}", catalog::markdown_table());
        } else {
            print!("{}", sadf::build_script(None, None));
        }
        return ExitCode::SUCCESS;
    }

    match session::run(&cli, &rep) {
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
