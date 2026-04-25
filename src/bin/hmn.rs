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

    // Binary crate's default tracing target is `hmn`, but the configured
    // EnvFilter only knows about `hypomnema=*`. Tag the binary's events so
    // they ride the same filter as lib events.
    tracing::debug!(
        target: "hypomnema::hmn",
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
