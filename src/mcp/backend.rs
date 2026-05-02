use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::api::types::{
    ContentGetRequest, ContentGetResponse, ContentQueryJson, ContentSearchResponse,
    CreateVaultRequest, FilesystemQueryJson, FilesystemSearchResponse, RescanResponseJson,
    SemanticQueryJson, SemanticSearchResponse, TerminateVaultResponse, VaultListResponse,
    VaultRowJson,
};
use crate::client::{DaemonClient, is_connect_error};

#[async_trait]
pub trait HypomnemaBackend: Send + Sync + 'static {
    async fn search_filesystem(&self, q: &FilesystemQueryJson) -> Result<FilesystemSearchResponse>;

    async fn search_content(&self, q: &ContentQueryJson) -> Result<ContentSearchResponse>;

    async fn search_semantic(&self, q: &SemanticQueryJson) -> Result<SemanticSearchResponse>;

    async fn content_get(&self, req: &ContentGetRequest) -> Result<ContentGetResponse>;

    async fn list_vaults(&self) -> Result<VaultListResponse>;

    async fn get_vault(&self, name_or_id: &str) -> Result<VaultRowJson>;

    async fn create_vault(&self, req: &CreateVaultRequest) -> Result<VaultRowJson>;

    async fn pause_vault(&self, name_or_id: &str) -> Result<VaultRowJson>;

    async fn resume_vault(&self, name_or_id: &str) -> Result<VaultRowJson>;

    async fn reset_vault(&self, name_or_id: &str, rebuild: bool) -> Result<VaultRowJson>;

    async fn rename_vault(&self, name_or_id: &str, new_name: &str) -> Result<VaultRowJson>;

    async fn rescan_vault(&self, name_or_id: &str) -> Result<RescanResponseJson>;

    async fn terminate_vault(&self, name_or_id: &str) -> Result<TerminateVaultResponse>;

    fn is_connect_error(&self, err: &anyhow::Error) -> bool;

    // Synthesize the `daemon_unreachable` envelope for this backend's transport.
    // Only called when `is_connect_error` returns `true`; backends whose
    // `is_connect_error` always returns `false` (e.g. in-process) never hit this.
    fn daemon_unreachable_envelope(&self, err: &anyhow::Error) -> Value;
}

#[async_trait]
impl HypomnemaBackend for DaemonClient {
    async fn search_filesystem(&self, q: &FilesystemQueryJson) -> Result<FilesystemSearchResponse> {
        DaemonClient::search_filesystem(self, q).await
    }

    async fn search_content(&self, q: &ContentQueryJson) -> Result<ContentSearchResponse> {
        DaemonClient::search_content(self, q).await
    }

    async fn search_semantic(&self, q: &SemanticQueryJson) -> Result<SemanticSearchResponse> {
        DaemonClient::search_semantic(self, q).await
    }

    async fn content_get(&self, req: &ContentGetRequest) -> Result<ContentGetResponse> {
        DaemonClient::content_get(self, req).await
    }

    async fn list_vaults(&self) -> Result<VaultListResponse> {
        DaemonClient::list_vaults(self).await
    }

    async fn get_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        DaemonClient::get_vault(self, name_or_id).await
    }

    async fn create_vault(&self, req: &CreateVaultRequest) -> Result<VaultRowJson> {
        DaemonClient::create_vault(self, req).await
    }

    async fn pause_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        DaemonClient::pause_vault(self, name_or_id).await
    }

    async fn resume_vault(&self, name_or_id: &str) -> Result<VaultRowJson> {
        DaemonClient::resume_vault(self, name_or_id).await
    }

    async fn reset_vault(&self, name_or_id: &str, rebuild: bool) -> Result<VaultRowJson> {
        DaemonClient::reset_vault(self, name_or_id, rebuild).await
    }

    async fn rename_vault(&self, name_or_id: &str, new_name: &str) -> Result<VaultRowJson> {
        DaemonClient::rename_vault(self, name_or_id, new_name).await
    }

    async fn rescan_vault(&self, name_or_id: &str) -> Result<RescanResponseJson> {
        DaemonClient::rescan_vault(self, name_or_id).await
    }

    async fn terminate_vault(&self, name_or_id: &str) -> Result<TerminateVaultResponse> {
        DaemonClient::terminate_vault(self, name_or_id).await
    }

    fn is_connect_error(&self, err: &anyhow::Error) -> bool {
        is_connect_error(err)
    }

    fn daemon_unreachable_envelope(&self, err: &anyhow::Error) -> Value {
        super::server::daemon_unreachable_envelope(self.base_url(), err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_impl<T: HypomnemaBackend>(_: &T) {}

    #[test]
    fn daemon_client_implements_hypomnema_backend() {
        let cfg = crate::config::Config::default_for_smoke_test(std::path::PathBuf::from(
            "/tmp/hypomnema-backend-trait-test",
        ));
        let client = DaemonClient::from_config(&cfg, None).unwrap();
        assert_impl(&client);
    }
}
