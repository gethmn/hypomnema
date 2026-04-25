use std::process::ExitCode;

use clap::Parser;

use hypomnema::cli::{Cli, Command};
use hypomnema::config::Config;
use hypomnema::logging::{self, BinaryKind};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match Config::load(cli.config.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("hmn: configuration error: {e:#}");
            return ExitCode::from(3);
        }
    };

    if let Err(e) = logging::init(&config.logging, cli.verbose, BinaryKind::Hmn) {
        eprintln!("hmn: error: {e:#}");
        return ExitCode::from(1);
    }

    tracing::debug!(
        daemon_url = ?cli.daemon_url,
        json = cli.json,
        "hmn: parsed CLI"
    );

    match cli.command {
        Command::Search { .. } | Command::Status => {
            eprintln!("hmn: not implemented yet (lands in step 5)");
            ExitCode::from(1)
        }
    }
}
