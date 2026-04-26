use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use hypomnema::cli::{Cli, Command, SearchMode};
use hypomnema::client::{
    ContentQueryJson, ContentResultJson, ContentSearchResponse, DaemonClient, FilesystemQueryJson,
    FilesystemResultJson, FilesystemSearchResponse, SemanticQueryJson, SemanticResultJson,
    SemanticSearchResponse, StatusResponse, is_connect_error,
};
use hypomnema::config::Config;
use hypomnema::logging::{self, BinaryKind};

#[tokio::main]
async fn main() -> ExitCode {
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

    let result = match cli.command {
        Command::Search { mode } => match mode {
            SearchMode::Filesystem {
                query,
                prefix,
                limit,
            } => {
                cmd_search_filesystem(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                )
                .await
            }
            SearchMode::Content {
                query,
                prefix,
                limit,
            } => {
                cmd_search_content(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                )
                .await
            }
            SearchMode::Semantic {
                query,
                prefix,
                limit,
            } => {
                cmd_search_semantic(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                )
                .await
            }
        },
        Command::Status => cmd_status(&config, cli.daemon_url.as_deref(), cli.json).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if is_connect_error(&e) {
                eprintln!("hmn: daemon not reachable: {e:#}");
                ExitCode::from(4)
            } else {
                eprintln!("hmn: error: {e:#}");
                ExitCode::from(1)
            }
        }
    }
}

async fn cmd_search_filesystem(
    config: &Config,
    override_url: Option<&str>,
    json: bool,
    query: String,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let req = FilesystemQueryJson {
        prefix,
        glob: Some(query),
        max_depth: None,
        limit,
    };
    let resp = client.search_filesystem(&req).await?;
    if json {
        print_json(&resp)?;
    } else {
        render_filesystem_text(&resp);
    }
    Ok(())
}

async fn cmd_search_content(
    config: &Config,
    override_url: Option<&str>,
    json: bool,
    query: String,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let req = ContentQueryJson {
        query,
        regex: false,
        case_sensitive: false,
        prefix,
        include_matches: true,
        max_matches_per_file: None,
        limit,
    };
    let resp = client.search_content(&req).await?;
    if json {
        print_json(&resp)?;
    } else {
        render_content_text(&resp);
    }
    Ok(())
}

async fn cmd_search_semantic(
    config: &Config,
    override_url: Option<&str>,
    json: bool,
    query: String,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let req = SemanticQueryJson {
        query,
        prefix,
        limit,
        min_similarity: None,
    };
    let resp = client.search_semantic(&req).await?;
    if json {
        print_json(&resp)?;
    } else {
        print!("{}", render_semantic_text(&resp));
    }
    Ok(())
}

async fn cmd_status(config: &Config, override_url: Option<&str>, json: bool) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let resp = client.status().await?;
    if json {
        print_json(&resp)?;
    } else {
        render_status_text(&resp);
    }
    Ok(())
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    println!("{text}");
    Ok(())
}

fn render_filesystem_text(resp: &FilesystemSearchResponse) {
    for r in &resp.results {
        println!("{}", filesystem_line(r));
    }
    if resp.truncated {
        println!("(truncated; raise --limit)");
    }
}

fn filesystem_line(r: &FilesystemResultJson) -> String {
    format!("{}  {} bytes  {}", r.path, r.size, r.mtime)
}

fn render_content_text(resp: &ContentSearchResponse) {
    let mut first = true;
    for r in &resp.results {
        if !first {
            println!();
        }
        first = false;
        print_content_block(r);
    }
    if resp.truncated {
        if !first {
            println!();
        }
        println!("(truncated; raise --limit)");
    }
}

fn print_content_block(r: &ContentResultJson) {
    println!("{} ({} matches)", r.path, r.match_count);
    for m in &r.matches {
        println!("  {}: {}", m.line, m.text);
    }
}

fn render_semantic_text(resp: &SemanticSearchResponse) -> String {
    let mut out = String::new();
    let mut first = true;
    for r in &resp.results {
        if !first {
            out.push('\n');
        }
        first = false;
        append_semantic_block(&mut out, r);
    }
    if let Some(h) = &resp.hint {
        if !first {
            out.push('\n');
        }
        out.push_str(&format!("({h})\n"));
    }
    out
}

fn append_semantic_block(out: &mut String, r: &SemanticResultJson) {
    out.push_str(&format!("{}  (score: {:.2})\n", r.file_path, r.score));
    let segments: Vec<&str> = r
        .heading_path
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.as_str())
        .collect();
    if !segments.is_empty() {
        out.push_str(&format!("  > {}\n", segments.join(" / ")));
    }
    out.push_str(&format!("  {}\n", r.text));
}

fn render_status_text(resp: &StatusResponse) {
    let last = resp.last_indexed_at.as_deref().unwrap_or("never");
    println!("vault:         {}", resp.vault);
    println!("indexed files: {}", resp.indexed_file_count);
    println!("last indexed:  {last}");
    println!(
        "outbox:        {} ({})",
        resp.outbox.path,
        human_bytes(resp.outbox.size_bytes)
    );
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = n as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_picks_right_unit() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
    }

    fn semantic_result(
        file_path: &str,
        score: f32,
        heading_path: Vec<&str>,
        text: &str,
    ) -> SemanticResultJson {
        SemanticResultJson {
            score,
            file_path: file_path.to_string(),
            chunk_index: 0,
            heading_path: heading_path.into_iter().map(String::from).collect(),
            text: text.to_string(),
            vault: None,
        }
    }

    #[test]
    fn render_semantic_text_includes_score_and_heading_path() {
        let resp = SemanticSearchResponse {
            results: vec![semantic_result(
                "notes/databases/pgvector.md",
                0.82,
                vec!["Architecture", "Indexing"],
                "Pgvector supports HNSW indexes.",
            )],
            hint: None,
        };
        let out = render_semantic_text(&resp);
        assert!(
            out.contains("notes/databases/pgvector.md  (score: 0.82)"),
            "missing path+score header in:\n{out}"
        );
        assert!(
            out.contains("  > Architecture / Indexing"),
            "missing joined heading_path in:\n{out}"
        );
        assert!(
            out.contains("  Pgvector supports HNSW indexes."),
            "missing body in:\n{out}"
        );
    }

    #[test]
    fn render_semantic_text_filters_empty_heading_segments() {
        let resp = SemanticSearchResponse {
            results: vec![semantic_result(
                "notes/orphan.md",
                0.5,
                vec!["Setup", "", "Prereqs"],
                "body",
            )],
            hint: None,
        };
        let out = render_semantic_text(&resp);
        assert!(
            out.contains("  > Setup / Prereqs"),
            "expected filtered heading_path 'Setup / Prereqs' in:\n{out}"
        );
        assert!(
            !out.contains(" /  / "),
            "expected empty segments dropped (no double-separator) in:\n{out}"
        );
    }

    #[test]
    fn render_semantic_text_renders_hint_when_present() {
        let resp = SemanticSearchResponse {
            results: vec![],
            hint: Some("semantic index is building".to_string()),
        };
        let out = render_semantic_text(&resp);
        assert!(
            out.contains("(semantic index is building)"),
            "expected hint suffix in:\n{out}"
        );
    }

    #[test]
    fn filesystem_line_has_path_size_mtime_layout() {
        let r = FilesystemResultJson {
            path: "notes/a.md".to_string(),
            size: 123,
            mtime: "2026-04-01T00:00:00Z".to_string(),
            content_hash: "sha256:00".to_string(),
            vault: None,
        };
        assert_eq!(
            filesystem_line(&r),
            "notes/a.md  123 bytes  2026-04-01T00:00:00Z"
        );
    }
}
