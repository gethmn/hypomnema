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
    /// Serve the MCP surface over stdio against a running `hmnd` daemon.
    /// Intended to be invoked by MCP-capable agent hosts (Claude Code,
    /// Iris). Process exits when its parent (the host) closes stdin.
    Mcp,
    /// Manage vaults (create / list / status / terminate).
    Vault {
        #[command(subcommand)]
        op: VaultOp,
    },
}

#[derive(Debug, Subcommand)]
pub enum VaultOp {
    /// Create a new vault.
    Create {
        /// Path to the vault directory. Must exist and be canonicalizable.
        path: PathBuf,
        /// Vault name. Defaults to config's default_vault_name.
        #[arg(long)]
        name: Option<String>,
    },
    /// List all registered vaults.
    List,
    /// Show details for a single vault.
    Status {
        /// Vault name or surrogate id. Defaults to default_vault_name when omitted.
        target: Option<String>,
    },
    /// Terminate a vault: stop its runner, delete its registry row, and remove its per-vault state.
    Terminate {
        /// Vault name or surrogate id.
        target: String,
        /// Skip the destructive-op confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Pause a vault: stop its runner; runtime state preserved.
    Pause {
        /// Vault name or surrogate id.
        target: String,
    },
    /// Resume a paused or errored vault: re-spawn its runner.
    Resume {
        /// Vault name or surrogate id.
        target: String,
    },
    /// Reset a vault: clear `last_error`. With `--rebuild`, also drop and rebuild chunks.
    Reset {
        /// Vault name or surrogate id.
        target: String,
        /// Drop and rebuild chunks + chunks_vec; preserves files + outbox.
        #[arg(long)]
        rebuild: bool,
        /// Skip the destructive-op confirmation prompt (required for --rebuild).
        #[arg(long)]
        yes: bool,
    },
    /// Rename a vault.
    Rename {
        /// Vault name or surrogate id.
        target: String,
        /// New vault name.
        #[arg(long, value_name = "NEW_NAME")]
        new_name: String,
    },
    /// Rescan a vault: force the watcher's debouncer to walk every file and re-index.
    Rescan {
        /// Vault name or surrogate id.
        target: String,
        /// Skip the destructive-op confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Stream live change events from one vault (or all active vaults with --all).
    Watch {
        /// Vault name or surrogate id. Defaults to config's default_vault_name.
        /// Ignored when --all is set.
        target: Option<String>,
        /// Watch all active vaults instead of a single vault.
        #[arg(long)]
        all: bool,
    },
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
        /// Restrict the search to a subset of vaults (comma-separated names or ids).
        /// Repeating the flag also works. Omitting queries all active vaults.
        #[arg(long, value_name = "NAME_OR_ID", value_delimiter = ',')]
        vaults: Vec<String>,
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
        /// Restrict the search to a subset of vaults (comma-separated names or ids).
        #[arg(long, value_name = "NAME_OR_ID", value_delimiter = ',')]
        vaults: Vec<String>,
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
        /// Restrict the search to a subset of vaults (comma-separated names or ids).
        #[arg(long, value_name = "NAME_OR_ID", value_delimiter = ',')]
        vaults: Vec<String>,
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
    fn parses_mcp_subcommand() {
        let cli = Cli::try_parse_from(["hmn", "mcp"]).expect("parses");
        assert!(matches!(cli.command, Command::Mcp));
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
                        vaults,
                    },
            } => {
                assert_eq!(query, "notes/**/*.md");
                assert!(prefix.is_none());
                assert!(limit.is_none());
                assert!(vaults.is_empty());
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
                        vaults,
                    },
            } => {
                assert_eq!(query, "pgvector");
                assert_eq!(prefix.as_deref(), Some("notes"));
                assert_eq!(limit, Some(25));
                assert!(vaults.is_empty());
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
    fn parses_search_filesystem_with_vaults_comma_separated() {
        // Per spec § Cross-Vault Search Semantics § `vaults` filter, the
        // CLI accepts comma-separated names/ids. clap's `value_delimiter`
        // splits one occurrence of the flag.
        let cli = Cli::try_parse_from([
            "hmn",
            "search",
            "filesystem",
            "**/*.md",
            "--vaults",
            "alpha,bravo",
        ])
        .expect("parses");
        match cli.command {
            Command::Search {
                mode: SearchMode::Filesystem { vaults, .. },
            } => assert_eq!(vaults, vec!["alpha".to_string(), "bravo".to_string()]),
            _ => panic!("expected Search/Filesystem"),
        }
    }

    #[test]
    fn parses_search_content_with_vaults_repeated_flag() {
        // Repeating the flag also accumulates entries — confirms that the
        // value-delimiter usage doesn't cap the flag at a single occurrence.
        let cli = Cli::try_parse_from([
            "hmn", "search", "content", "needle", "--vaults", "alpha", "--vaults", "bravo",
        ])
        .expect("parses");
        match cli.command {
            Command::Search {
                mode: SearchMode::Content { vaults, .. },
            } => assert_eq!(vaults, vec!["alpha".to_string(), "bravo".to_string()]),
            _ => panic!("expected Search/Content"),
        }
    }

    #[test]
    fn parses_search_semantic_with_vaults() {
        let cli = Cli::try_parse_from([
            "hmn",
            "search",
            "semantic",
            "topic",
            "--vaults",
            "personal,work",
        ])
        .expect("parses");
        match cli.command {
            Command::Search {
                mode: SearchMode::Semantic { vaults, .. },
            } => assert_eq!(vaults, vec!["personal".to_string(), "work".to_string()]),
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

    #[test]
    fn parses_vault_create_with_path() {
        let cli = Cli::try_parse_from(["hmn", "vault", "create", "/tmp/foo"])
            .expect("vault create parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Create { path, name },
            } => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/foo"));
                assert!(name.is_none());
            }
            _ => panic!("expected Vault/Create"),
        }
    }

    #[test]
    fn parses_vault_create_with_name_and_path() {
        let cli = Cli::try_parse_from(["hmn", "vault", "create", "--name", "personal", "/tmp/foo"])
            .expect("vault create with name parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Create { path, name },
            } => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/foo"));
                assert_eq!(name.as_deref(), Some("personal"));
            }
            _ => panic!("expected Vault/Create"),
        }
    }

    #[test]
    fn parses_vault_list() {
        let cli = Cli::try_parse_from(["hmn", "vault", "list"]).expect("vault list parses");
        assert!(matches!(cli.command, Command::Vault { op: VaultOp::List }));
    }

    #[test]
    fn parses_vault_status_with_target() {
        let cli = Cli::try_parse_from(["hmn", "vault", "status", "personal"])
            .expect("vault status with target parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Status { target },
            } => assert_eq!(target.as_deref(), Some("personal")),
            _ => panic!("expected Vault/Status"),
        }
    }

    #[test]
    fn parses_vault_status_without_target() {
        let cli =
            Cli::try_parse_from(["hmn", "vault", "status"]).expect("vault status parses bare");
        match cli.command {
            Command::Vault {
                op: VaultOp::Status { target },
            } => assert!(target.is_none()),
            _ => panic!("expected Vault/Status"),
        }
    }

    #[test]
    fn parses_vault_terminate_with_yes() {
        let cli = Cli::try_parse_from(["hmn", "vault", "terminate", "personal", "--yes"])
            .expect("vault terminate --yes parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Terminate { target, yes },
            } => {
                assert_eq!(target, "personal");
                assert!(yes);
            }
            _ => panic!("expected Vault/Terminate"),
        }
    }

    #[test]
    fn parses_vault_terminate_without_yes() {
        let cli = Cli::try_parse_from(["hmn", "vault", "terminate", "personal"])
            .expect("vault terminate parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Terminate { target, yes },
            } => {
                assert_eq!(target, "personal");
                assert!(!yes);
            }
            _ => panic!("expected Vault/Terminate"),
        }
    }

    #[test]
    fn missing_vault_op_is_an_error() {
        let err = Cli::try_parse_from(["hmn", "vault"]).expect_err("must require an op");
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::MissingSubcommand
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        ));
    }

    #[test]
    fn vault_terminate_without_target_is_an_error() {
        let err = Cli::try_parse_from(["hmn", "vault", "terminate"])
            .expect_err("terminate requires target");
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        ));
    }

    #[test]
    fn parses_vault_pause_with_target() {
        let cli =
            Cli::try_parse_from(["hmn", "vault", "pause", "personal"]).expect("vault pause parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Pause { target },
            } => assert_eq!(target, "personal"),
            _ => panic!("expected Vault/Pause"),
        }
    }

    #[test]
    fn parses_vault_resume_with_target() {
        let cli = Cli::try_parse_from(["hmn", "vault", "resume", "personal"])
            .expect("vault resume parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Resume { target },
            } => assert_eq!(target, "personal"),
            _ => panic!("expected Vault/Resume"),
        }
    }

    #[test]
    fn parses_vault_reset_with_target() {
        let cli =
            Cli::try_parse_from(["hmn", "vault", "reset", "personal"]).expect("vault reset parses");
        match cli.command {
            Command::Vault {
                op:
                    VaultOp::Reset {
                        target,
                        rebuild,
                        yes,
                    },
            } => {
                assert_eq!(target, "personal");
                assert!(!rebuild);
                assert!(!yes);
            }
            _ => panic!("expected Vault/Reset"),
        }
    }

    #[test]
    fn parses_vault_reset_with_rebuild_and_yes() {
        let cli = Cli::try_parse_from(["hmn", "vault", "reset", "personal", "--rebuild", "--yes"])
            .expect("vault reset --rebuild --yes parses");
        match cli.command {
            Command::Vault {
                op:
                    VaultOp::Reset {
                        target,
                        rebuild,
                        yes,
                    },
            } => {
                assert_eq!(target, "personal");
                assert!(rebuild);
                assert!(yes);
            }
            _ => panic!("expected Vault/Reset"),
        }
    }

    #[test]
    fn parses_vault_rename_with_new_name() {
        let cli = Cli::try_parse_from(["hmn", "vault", "rename", "old", "--new-name", "fresh"])
            .expect("vault rename parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Rename { target, new_name },
            } => {
                assert_eq!(target, "old");
                assert_eq!(new_name, "fresh");
            }
            _ => panic!("expected Vault/Rename"),
        }
    }

    #[test]
    fn parses_vault_rescan_with_target_and_yes() {
        let cli = Cli::try_parse_from(["hmn", "vault", "rescan", "personal", "--yes"])
            .expect("vault rescan parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Rescan { target, yes },
            } => {
                assert_eq!(target, "personal");
                assert!(yes);
            }
            _ => panic!("expected Vault/Rescan"),
        }
    }

    #[test]
    fn parses_vault_watch_with_explicit_target() {
        let cli = Cli::try_parse_from(["hmn", "vault", "watch", "personal"])
            .expect("vault watch with target parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Watch { target, all },
            } => {
                assert_eq!(target.as_deref(), Some("personal"));
                assert!(!all);
            }
            _ => panic!("expected Vault/Watch"),
        }
    }

    #[test]
    fn parses_vault_watch_without_target_defaults_to_none() {
        let cli = Cli::try_parse_from(["hmn", "vault", "watch"]).expect("vault watch bare parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Watch { target, all },
            } => {
                assert!(target.is_none());
                assert!(!all);
            }
            _ => panic!("expected Vault/Watch"),
        }
    }

    #[test]
    fn parses_vault_watch_all_flag() {
        let cli = Cli::try_parse_from(["hmn", "vault", "watch", "--all"])
            .expect("vault watch --all parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Watch { target, all },
            } => {
                assert!(target.is_none());
                assert!(all);
            }
            _ => panic!("expected Vault/Watch"),
        }
    }

    #[test]
    fn parses_vault_watch_all_flag_with_ignored_target() {
        let cli = Cli::try_parse_from(["hmn", "vault", "watch", "personal", "--all"])
            .expect("vault watch target --all parses");
        match cli.command {
            Command::Vault {
                op: VaultOp::Watch { target, all },
            } => {
                // target is provided but --all overrides it at runtime
                assert_eq!(target.as_deref(), Some("personal"));
                assert!(all);
            }
            _ => panic!("expected Vault/Watch"),
        }
    }
}
