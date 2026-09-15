//! 邮箱机器人集成（finance domain：身份凭证资产）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/email/` 路由组：
//! 集成状态聚合（platform 可选过滤，空串 = 全部提供商） / 凭证 CRUD / 默认凭证。
//!
//! 邮箱机器人 = 用户自建代理邮箱（阶段一仅出站推送，IMAP 入站为二期）。

use common::api::{
    CreateEmailBotCredentialRequest, CreateEmailBotCredentialResponse,
    EmailIntegrationStatusResponse, SetDefaultEmailBotCredentialRequest,
    SetDefaultEmailBotCredentialResponse, UpdateEmailBotCredentialRequest,
    UpdateEmailBotCredentialResponse,
};

use super::{ApiError, api_delete, api_get_or_default, api_patch, api_post};

const BASE: &str = "/api/v1/finance/identity/email";

// ===== 集成状态聚合 =====

/// 获取当前用户邮箱机器人凭证快照（platform 空串返回全部提供商，渠道创建下拉使用）
///
/// ⚠️ `platform` **必须出现在 query 里**，空串也要写成 `?platform=`（`platform` 空 = 不过滤）。
/// 后端 `EmailIntegrationStatusRequest.platform` 是必填 `String`（query 参数走
/// `serde_json::from_value` 反序列化），**整段 query 缺失**会直接 400：
/// `invalid_request: query 参数解析失败: missing field \`platform\``。
/// 别为「空值」做「省略参数」的优化——那是把空串当成缺字段。
pub async fn get_email_integration_status(
    platform: &str,
) -> Result<EmailIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status?platform={}", BASE, platform)).await
}

// ===== 凭证 CRUD =====

/// 创建邮箱机器人凭证（密码/授权码加密落库，永不回显）
pub async fn create_email_bot_credential(
    req: CreateEmailBotCredentialRequest,
) -> Result<CreateEmailBotCredentialResponse, ApiError> {
    api_post(&format!("{}/credentials", BASE), &req).await
}

/// 更新邮箱机器人凭证（各字段留空保留原值，端口留空/0 不变）
pub async fn update_email_bot_credential(
    req: UpdateEmailBotCredentialRequest,
) -> Result<UpdateEmailBotCredentialResponse, ApiError> {
    api_patch(&format!("{}/credentials/{}", BASE, req.id), &req).await
}

/// 删除邮箱机器人凭证
pub async fn delete_email_bot_credential(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("{}/credentials/{}", BASE, id)).await
}

/// 设置默认邮箱机器人凭证（同 provider 下多条凭证时渠道出站优先；空串取消默认）
pub async fn set_default_email_bot_credential(
    platform: &str,
    credential_id: &str,
) -> Result<SetDefaultEmailBotCredentialResponse, ApiError> {
    api_post(
        &format!("{}/credentials/default", BASE),
        &SetDefaultEmailBotCredentialRequest {
            platform: platform.to_string(),
            credential_id: credential_id.to_string(),
        },
    )
    .await
}
