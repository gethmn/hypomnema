use anyhow::{Context, Result};
use rmcp::ServiceExt;

mod server;

pub use server::{HypomnemaMcpServer, daemon_unreachable_envelope};

pub async fn serve_stdio(server: HypomnemaMcpServer) -> Result<()> {
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .context("initializing MCP service over stdio")?;
    service.waiting().await.context("waiting on MCP service")?;
    Ok(())
}
