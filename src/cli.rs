use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "hmn",
    version,
    about = "Hypomnema CLI client",
    arg_required_else_help = true
)]
pub struct Cli {
    #[arg(short, long, value_name = "PATH", global = true)]
    pub config: Option<PathBuf>,

    #[arg(long, value_name = "URL", global = true)]
    pub daemon_url: Option<String>,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Search the running daemon.
    Search {
        #[command(subcommand)]
        mode: SearchMode,
    },
    /// Report daemon health.
    Status,
}

#[derive(Debug, Subcommand)]
pub enum SearchMode {
    /// Glob over vault file paths.
    Filesystem {
        /// Glob pattern over vault paths.
        query: String,
        /// Restrict results to a vault subdirectory.
        #[arg(long, value_name = "PATH")]
        prefix: Option<String>,
        /// Max results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Substring/regex over file contents.
    Content {
        /// Substring or regex to match.
        query: String,
        /// Restrict results to a vault subdirectory.
        #[arg(long, value_name = "PATH")]
        prefix: Option<String>,
        /// Max results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Natural-language semantic search.
    Semantic {
        /// Natural-language query.
        query: String,
        /// Restrict results to a vault subdirectory.
        #[arg(long, value_name = "PATH")]
        prefix: Option<String>,
        /// Max results.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_status() {
        let cli = Cli::try_parse_from(["hmn", "status"]).expect("status parses");
        assert!(matches!(cli.command, Command::Status));
    }

    #[test]
    fn parses_search_filesystem_with_query() {
        let cli =
            Cli::try_parse_from(["hmn", "search", "filesystem", "notes/**/*.md"]).expect("parses");
        match cli.command {
            Command::Search {
                mode:
                    SearchMode::Filesystem {
                        query,
                        prefix,
                        limit,
                    },
            } => {
                assert_eq!(query, "notes/**/*.md");
                assert!(prefix.is_none());
                assert!(limit.is_none());
            }
            _ => panic!("expected Search/Filesystem"),
        }
    }

    #[test]
    fn parses_search_content_with_options() {
        let cli = Cli::try_parse_from([
            "hmn", "search", "content", "pgvector", "--prefix", "notes", "--limit", "25",
        ])
        .expect("parses");
        match cli.command {
            Command::Search {
                mode:
                    SearchMode::Content {
                        query,
                        prefix,
                        limit,
                    },
            } => {
                assert_eq!(query, "pgvector");
                assert_eq!(prefix.as_deref(), Some("notes"));
                assert_eq!(limit, Some(25));
            }
            _ => panic!("expected Search/Content"),
        }
    }

    #[test]
    fn parses_search_semantic() {
        let cli = Cli::try_parse_from(["hmn", "search", "semantic", "how do indexes work"])
            .expect("parses");
        match cli.command {
            Command::Search {
                mode: SearchMode::Semantic { query, .. },
            } => {
                assert_eq!(query, "how do indexes work");
            }
            _ => panic!("expected Search/Semantic"),
        }
    }

    #[test]
    fn global_flags_parse() {
        let cli = Cli::try_parse_from([
            "hmn",
            "--config",
            "/tmp/x.toml",
            "--daemon-url",
            "http://127.0.0.1:9999",
            "-vv",
            "--json",
            "status",
        ])
        .expect("parses");
        assert_eq!(
            cli.config.as_deref(),
            Some(std::path::Path::new("/tmp/x.toml"))
        );
        assert_eq!(cli.daemon_url.as_deref(), Some("http://127.0.0.1:9999"));
        assert_eq!(cli.verbose, 2);
        assert!(cli.json);
    }

    #[test]
    fn verbose_flag_after_subcommand_thanks_to_global() {
        let cli = Cli::try_parse_from(["hmn", "status", "-v"]).expect("parses");
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn missing_search_mode_is_an_error() {
        let err = Cli::try_parse_from(["hmn", "search"]).expect_err("must require a mode");
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::MissingSubcommand
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        ));
    }

    #[test]
    fn unknown_search_mode_is_an_error() {
        let err = Cli::try_parse_from(["hmn", "search", "regex", "foo"]).expect_err("unknown mode");
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::InvalidSubcommand | clap::error::ErrorKind::UnknownArgument
        ));
    }

    #[test]
    fn bare_invocation_renders_help() {
        let err = Cli::try_parse_from(["hmn"]).expect_err("bare hmn shows help");
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }
}
