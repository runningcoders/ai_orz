//! 通用 API Token 集成（finance domain：身份凭证资产）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/generic-token/` 路由组：
//! 集成状态聚合（按 platform 过滤） / 凭证 CRUD / 默认凭证。
//!
//! 适用于所有单字段 API Key 类平台（tavily / doubao_search / 未来平台）。

use common::api::{
    CreateGenericTokenCredentialRequest, CreateGenericTokenCredentialResponse,
    GenericTokenIntegrationStatusResponse, SetDefaultGenericTokenCredentialRequest,
    SetDefaultGenericTokenCredentialResponse, UpdateGenericTokenCredentialRequest,
    UpdateGenericTokenCredentialResponse,
};

use super::{ApiError, api_delete, api_get_or_default, api_patch, api_post};

const BASE: &str = "/api/v1/finance/identity/generic-token";

// ===== 集成状态聚合 =====

/// 获取当前用户在指定 platform 下的通用 token 凭证状态
pub async fn get_generic_token_status(
    platform: &str,
) -> Result<GenericTokenIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status?platform={}", BASE, platform)).await
}

// ===== 凭证 CRUD =====

/// 创建通用 token 凭证（token 加密落库，永不回显）
pub async fn create_generic_token_credential(
    req: CreateGenericTokenCredentialRequest,
) -> Result<CreateGenericTokenCredentialResponse, ApiError> {
    api_post(&format!("{}/credentials", BASE), &req).await
}

/// 更新通用 token 凭证（token 留空保留原值）
pub async fn update_generic_token_credential(
    req: UpdateGenericTokenCredentialRequest,
) -> Result<UpdateGenericTokenCredentialResponse, ApiError> {
    api_patch(&format!("{}/credentials/{}", BASE, req.id), &req).await
}

/// 删除通用 token 凭证
pub async fn delete_generic_token_credential(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("{}/credentials/{}", BASE, id)).await
}

/// 设置默认通用 token 凭证（同 platform 下多条 token 时工具身份优先；空串取消默认）
pub async fn set_default_generic_token_credential(
    platform: &str,
    credential_id: &str,
) -> Result<SetDefaultGenericTokenCredentialResponse, ApiError> {
    api_post(
        &format!("{}/credentials/default", BASE),
        &SetDefaultGenericTokenCredentialRequest {
            platform: platform.to_string(),
            credential_id: credential_id.to_string(),
        },
    )
    .await
}
