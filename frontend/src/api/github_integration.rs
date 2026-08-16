//! GitHub 集成（finance domain：身份凭证资产）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/github/` 路由组：
//! 集成状态聚合 / 凭证 CRUD / 默认凭证。

use common::api::{
    CreateGithubCredentialRequest, CreateGithubCredentialResponse, GithubIntegrationStatusResponse,
    SetDefaultGithubCredentialRequest, SetDefaultGithubCredentialResponse,
    UpdateGithubCredentialRequest, UpdateGithubCredentialResponse,
};

use super::{ApiError, api_delete, api_get_or_default, api_post, api_put};

const BASE: &str = "/api/v1/finance/identity/github";

// ===== 集成状态聚合 =====

/// 获取当前用户 GitHub 集成状态（凭证快照 + gh 登录态实测）
pub async fn get_github_integration_status() -> Result<GithubIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status", BASE)).await
}

// ===== 凭证 CRUD =====

/// 手动录入创建 GitHub 凭证（PAT 加密落库，永不回显）
pub async fn create_github_credential(
    req: CreateGithubCredentialRequest,
) -> Result<CreateGithubCredentialResponse, ApiError> {
    api_post(&format!("{}/credentials", BASE), &req).await
}

/// 更新 GitHub 凭证（token 变更后 gh_cli 自动重新登录）
pub async fn update_github_credential(
    req: UpdateGithubCredentialRequest,
) -> Result<UpdateGithubCredentialResponse, ApiError> {
    api_put(&format!("{}/credentials/{}", BASE, req.id), &req).await
}

/// 删除 GitHub 凭证（生效凭证删除时联动清登录态）
pub async fn delete_github_credential(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("{}/credentials/{}", BASE, id)).await
}

/// 设置默认 GitHub 凭证（多条 token 时 gh_cli 身份优先；空串取消默认）
pub async fn set_default_github_credential(
    credential_id: &str,
) -> Result<SetDefaultGithubCredentialResponse, ApiError> {
    api_post(
        &format!("{}/credentials/default", BASE),
        &SetDefaultGithubCredentialRequest {
            credential_id: credential_id.to_string(),
        },
    )
    .await
}
