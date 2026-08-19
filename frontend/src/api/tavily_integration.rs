//! Tavily 集成（finance domain：身份凭证资产）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/tavily/` 路由组：
//! 集成状态聚合 / 凭证 CRUD / 默认凭证。

use common::api::{
    CreateTavilyCredentialRequest, CreateTavilyCredentialResponse,
    SetDefaultTavilyCredentialRequest, SetDefaultTavilyCredentialResponse,
    TavilyIntegrationStatusResponse, UpdateTavilyCredentialRequest, UpdateTavilyCredentialResponse,
};

use super::{ApiError, api_delete, api_get_or_default, api_post, api_put};

const BASE: &str = "/api/v1/finance/identity/tavily";

// ===== 集成状态聚合 =====

/// 获取当前用户 Tavily 集成状态（凭证快照 + 共享 key 配置状态）
pub async fn get_tavily_integration_status() -> Result<TavilyIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status", BASE)).await
}

// ===== 凭证 CRUD =====

/// 手动录入创建 Tavily 凭证（API key 加密落库，永不回显）
pub async fn create_tavily_credential(
    req: CreateTavilyCredentialRequest,
) -> Result<CreateTavilyCredentialResponse, ApiError> {
    api_post(&format!("{}/credentials", BASE), &req).await
}

/// 更新 Tavily 凭证（key 留空保留原值）
pub async fn update_tavily_credential(
    req: UpdateTavilyCredentialRequest,
) -> Result<UpdateTavilyCredentialResponse, ApiError> {
    api_put(&format!("{}/credentials/{}", BASE, req.id), &req).await
}

/// 删除 Tavily 凭证
pub async fn delete_tavily_credential(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("{}/credentials/{}", BASE, id)).await
}

/// 设置默认 Tavily 凭证（多条 key 时 tavily_search 身份优先；空串取消默认）
pub async fn set_default_tavily_credential(
    credential_id: &str,
) -> Result<SetDefaultTavilyCredentialResponse, ApiError> {
    api_post(
        &format!("{}/credentials/default", BASE),
        &SetDefaultTavilyCredentialRequest {
            credential_id: credential_id.to_string(),
        },
    )
    .await
}
