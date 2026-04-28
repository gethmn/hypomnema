use anyhow::{Context, Result};
use rmcp::ServiceExt;

pub mod backend;
pub mod backend_in_process;
mod server;

pub use backend::HypomnemaBackend;
pub use backend_in_process::InProcessBackend;
pub use server::{HypomnemaMcpServer, daemon_unreachable_envelope};

pub async fn serve_stdio(server: HypomnemaMcpServer) -> Result<()> {
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .context("initializing MCP service over stdio")?;
    service.waiting().await.context("waiting on MCP service")?;
    Ok(())
}
