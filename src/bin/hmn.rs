use std::io::{BufRead, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow};
use clap::Parser;

use hypomnema::cli::{Cli, Command, SearchMode, VaultOp};
use hypomnema::client::{
    ContentQueryJson, ContentResultJson, ContentSearchResponse, CreateVaultRequest, DaemonClient,
    FilesystemQueryJson, FilesystemResultJson, FilesystemSearchResponse, RescanResponseJson,
    SemanticQueryJson, SemanticResultJson, SemanticSearchResponse, StatusResponse,
    TerminateVaultResponse, VaultListResponse, VaultRowJson, is_connect_error,
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

    let kind = match &cli.command {
        Command::Mcp => BinaryKind::HmnMcp,
        _ => BinaryKind::Hmn,
    };
    if let Err(e) = logging::init(&config.logging, cli.verbose, kind) {
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
                vaults,
            } => {
                cmd_search_filesystem(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                    vaults,
                )
                .await
            }
            SearchMode::Content {
                query,
                prefix,
                limit,
                vaults,
            } => {
                cmd_search_content(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                    vaults,
                )
                .await
            }
            SearchMode::Semantic {
                query,
                prefix,
                limit,
                vaults,
            } => {
                cmd_search_semantic(
                    &config,
                    cli.daemon_url.as_deref(),
                    cli.json,
                    query,
                    prefix,
                    limit,
                    vaults,
                )
                .await
            }
        },
        Command::Status => cmd_status(&config, cli.daemon_url.as_deref(), cli.json).await,
        Command::Mcp => cmd_mcp(&config, cli.daemon_url.as_deref()).await,
        Command::Vault { op } => cmd_vault(&config, cli.daemon_url.as_deref(), cli.json, op).await,
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
    vaults: Vec<String>,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let req = FilesystemQueryJson {
        prefix,
        glob: Some(query),
        max_depth: None,
        limit,
        vaults: vaults_or_none(vaults),
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
    vaults: Vec<String>,
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
        vaults: vaults_or_none(vaults),
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
    vaults: Vec<String>,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    let req = SemanticQueryJson {
        query,
        prefix,
        limit,
        min_similarity: None,
        vaults: vaults_or_none(vaults),
    };
    let resp = client.search_semantic(&req).await?;
    if json {
        print_json(&resp)?;
    } else {
        print!("{}", render_semantic_text(&resp));
    }
    Ok(())
}

fn vaults_or_none(v: Vec<String>) -> Option<Vec<String>> {
    if v.is_empty() { None } else { Some(v) }
}

async fn cmd_mcp(config: &Config, override_url: Option<&str>) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)
        .context("constructing DaemonClient for mcp subcommand")?;
    let server = hypomnema::mcp::HypomnemaMcpServer {
        backend: std::sync::Arc::new(client),
        default_vault_name: config.default_vault_name.clone(),
        enable_write_tools: config.mcp.enable_write_tools,
    };
    hypomnema::mcp::serve_stdio(server)
        .await
        .context("serving MCP over stdio")
}

async fn cmd_vault(
    config: &Config,
    override_url: Option<&str>,
    json: bool,
    op: VaultOp,
) -> Result<()> {
    let client = DaemonClient::from_config(config, override_url)?;
    match op {
        VaultOp::Create { path, name } => {
            let req = CreateVaultRequest {
                name,
                path: path.display().to_string(),
            };
            let row = client.create_vault(&req).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::List => {
            let resp = client.list_vaults().await?;
            if json {
                print_json(&resp)?;
            } else {
                render_vault_list(&resp);
            }
        }
        VaultOp::Status { target } => {
            let target = resolve_target(config, target.as_deref())?;
            let row = client.get_vault(&target).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::Terminate { target, yes } => {
            if !yes && !confirm_terminate(&target, std::io::stdin().lock(), &mut std::io::stderr())?
            {
                if json {
                    print_json(&serde_json::json!({ "terminated": false, "aborted": true }))?;
                } else {
                    println!("aborted");
                }
                return Ok(());
            }
            let resp = client.terminate_vault(&target).await?;
            if json {
                print_json(&resp)?;
            } else {
                render_terminate(&resp);
            }
        }
        VaultOp::Pause { target } => {
            let row = client.pause_vault(&target).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::Resume { target } => {
            let row = client.resume_vault(&target).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::Reset {
            target,
            rebuild,
            yes,
        } => {
            if rebuild
                && !yes
                && !confirm_reset_rebuild(&target, std::io::stdin().lock(), &mut std::io::stderr())?
            {
                if json {
                    print_json(&serde_json::json!({ "reset": false, "aborted": true }))?;
                } else {
                    println!("aborted");
                }
                return Ok(());
            }
            let row = client.reset_vault(&target, rebuild).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::Rename { target, new_name } => {
            let row = client.rename_vault(&target, &new_name).await?;
            if json {
                print_json(&row)?;
            } else {
                render_vault_row(&row);
            }
        }
        VaultOp::Rescan { target, yes } => {
            if !yes && !confirm_rescan(&target, std::io::stdin().lock(), &mut std::io::stderr())? {
                if json {
                    print_json(&serde_json::json!({ "rescan": false, "aborted": true }))?;
                } else {
                    println!("aborted");
                }
                return Ok(());
            }
            let resp = client.rescan_vault(&target).await?;
            if json {
                print_json(&resp)?;
            } else {
                render_rescan(&resp);
            }
        }
    }
    Ok(())
}

fn resolve_target(config: &Config, target: Option<&str>) -> Result<String> {
    match target {
        Some(t) => Ok(t.to_string()),
        None => {
            let default = config.default_vault_name.trim();
            if default.is_empty() {
                Err(anyhow!(
                    "no target supplied and default_vault_name is empty in config"
                ))
            } else {
                Ok(default.to_string())
            }
        }
    }
}

/// Read a yes/no answer from `stdin`, prompting on `prompt_writer`. Returns
/// `true` only when the input begins with `y` or `Y`. EOF / empty input /
/// anything else means "no" — destructive ops require an explicit
/// affirmative.
fn confirm_terminate<R: BufRead, W: Write>(
    target: &str,
    reader: R,
    prompt_writer: &mut W,
) -> Result<bool> {
    confirm_yn(
        reader,
        prompt_writer,
        &format!("Terminate vault '{target}'? (y/N) "),
    )
}

fn confirm_reset_rebuild<R: BufRead, W: Write>(
    target: &str,
    reader: R,
    prompt_writer: &mut W,
) -> Result<bool> {
    confirm_yn(
        reader,
        prompt_writer,
        &format!("Reset vault '{target}' and rebuild chunks? (y/N) "),
    )
}

fn confirm_rescan<R: BufRead, W: Write>(
    target: &str,
    reader: R,
    prompt_writer: &mut W,
) -> Result<bool> {
    confirm_yn(
        reader,
        prompt_writer,
        &format!("Rescan vault '{target}'? This will re-emit outbox events. (y/N) "),
    )
}

fn confirm_yn<R: BufRead, W: Write>(
    mut reader: R,
    prompt_writer: &mut W,
    prompt: &str,
) -> Result<bool> {
    write!(prompt_writer, "{prompt}")?;
    prompt_writer.flush()?;
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(false);
    }
    let trimmed = line.trim_start();
    Ok(trimmed.chars().next().is_some_and(|c| c == 'y' || c == 'Y'))
}

fn render_vault_row(row: &VaultRowJson) {
    println!("id:         {}", row.id);
    println!("name:       {}", row.name);
    println!("path:       {}", row.path);
    println!("status:     {}", row.status);
    println!("created_at: {}", row.created_at);
    if let Some(err) = &row.last_error {
        println!("last_error: {err}");
    }
}

fn render_vault_list(resp: &VaultListResponse) {
    if resp.vaults.is_empty() {
        println!("(no vaults)");
        return;
    }
    let widths = column_widths(&resp.vaults);
    println!(
        "{:<id_w$}  {:<name_w$}  {:<status_w$}  {:<created_w$}  PATH",
        "ID",
        "NAME",
        "STATUS",
        "CREATED",
        id_w = widths.id,
        name_w = widths.name,
        status_w = widths.status,
        created_w = widths.created,
    );
    for row in &resp.vaults {
        println!(
            "{:<id_w$}  {:<name_w$}  {:<status_w$}  {:<created_w$}  {}",
            row.id,
            row.name,
            row.status,
            row.created_at,
            row.path,
            id_w = widths.id,
            name_w = widths.name,
            status_w = widths.status,
            created_w = widths.created,
        );
    }
}

struct ColumnWidths {
    id: usize,
    name: usize,
    status: usize,
    created: usize,
}

fn column_widths(rows: &[VaultRowJson]) -> ColumnWidths {
    let mut w = ColumnWidths {
        id: "ID".len(),
        name: "NAME".len(),
        status: "STATUS".len(),
        created: "CREATED".len(),
    };
    for row in rows {
        w.id = w.id.max(row.id.len());
        w.name = w.name.max(row.name.len());
        w.status = w.status.max(row.status.len());
        w.created = w.created.max(row.created_at.len());
    }
    w
}

fn render_terminate(resp: &TerminateVaultResponse) {
    println!("terminated: {}", resp.terminated);
    println!("id:         {}", resp.id);
}

fn render_rescan(resp: &RescanResponseJson) {
    render_vault_row(&resp.row);
    println!("rescan_initiated_at: {}", resp.rescan_initiated_at);
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
            vault_name: None,
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
            partial_results: None,
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
            partial_results: None,
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
            partial_results: None,
        };
        let out = render_semantic_text(&resp);
        assert!(
            out.contains("(semantic index is building)"),
            "expected hint suffix in:\n{out}"
        );
    }

    #[test]
    fn confirm_terminate_accepts_lowercase_y() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_terminate("personal", &b"y\n"[..], &mut sink).unwrap();
        assert!(answered);
        assert!(String::from_utf8_lossy(&sink).contains("Terminate vault 'personal'? (y/N) "));
    }

    #[test]
    fn confirm_terminate_accepts_uppercase_y() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_terminate("personal", &b"Yes\n"[..], &mut sink).unwrap();
        assert!(answered);
    }

    #[test]
    fn confirm_terminate_rejects_no() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_terminate("personal", &b"n\n"[..], &mut sink).unwrap();
        assert!(!answered);
    }

    #[test]
    fn confirm_terminate_rejects_empty_line() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_terminate("personal", &b"\n"[..], &mut sink).unwrap();
        assert!(!answered);
    }

    #[test]
    fn confirm_terminate_rejects_eof() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_terminate("personal", &b""[..], &mut sink).unwrap();
        assert!(!answered);
    }

    #[test]
    fn confirm_reset_rebuild_emits_rebuild_prompt() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_reset_rebuild("personal", &b"y\n"[..], &mut sink).unwrap();
        assert!(answered);
        assert!(
            String::from_utf8_lossy(&sink)
                .contains("Reset vault 'personal' and rebuild chunks? (y/N) "),
        );
    }

    #[test]
    fn confirm_rescan_emits_rescan_prompt() {
        let mut sink = Vec::<u8>::new();
        let answered = confirm_rescan("personal", &b"n\n"[..], &mut sink).unwrap();
        assert!(!answered);
        assert!(
            String::from_utf8_lossy(&sink)
                .contains("Rescan vault 'personal'? This will re-emit outbox events. (y/N) "),
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
            vault_name: None,
        };
        assert_eq!(
            filesystem_line(&r),
            "notes/a.md  123 bytes  2026-04-01T00:00:00Z"
        );
    }
}
