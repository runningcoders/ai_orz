//! 飞书集成（finance domain：身份凭证资产）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/lark/` 路由组：
//! 绑定快照聚合 / 凭证 CRUD / 默认凭证 / 用户 OAuth device flow / config init --new 自动绑定。

use common::api::{
    CreateLarkCredentialRequest, CreateLarkCredentialResponse, LarkAuthCompleteRequest,
    LarkAuthCompleteResponse, LarkAuthLogoutResponse, LarkAuthStartRequest, LarkAuthStartResponse,
    LarkAuthStatusResponse, LarkBindCancelResponse, LarkBindStartResponse, LarkBindStatusResponse,
    LarkIntegrationStatusResponse, SetDefaultLarkCredentialRequest,
    SetDefaultLarkCredentialResponse, UpdateLarkCredentialRequest, UpdateLarkCredentialResponse,
};

use super::{ApiError, api_delete, api_get, api_get_or_default, api_post, api_put};

const BASE: &str = "/api/v1/finance/identity/lark";

// ===== 绑定快照聚合 =====

/// 获取当前用户飞书集成绑定快照（凭证 + 引用渠道 + 用户授权状态）
pub async fn get_lark_integration_status() -> Result<LarkIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status", BASE)).await
}

// ===== 凭证 CRUD =====

/// 手动录入创建飞书应用凭证
pub async fn create_lark_credential(
    req: CreateLarkCredentialRequest,
) -> Result<CreateLarkCredentialResponse, ApiError> {
    api_post(&format!("{}/credentials", BASE), &req).await
}

/// 更新飞书应用凭证（关联渠道将重建联）
pub async fn update_lark_credential(
    req: UpdateLarkCredentialRequest,
) -> Result<UpdateLarkCredentialResponse, ApiError> {
    api_put(&format!("{}/credentials/{}", BASE, req.id), &req).await
}

/// 删除飞书应用凭证（有渠道引用时后端报 Conflict）
pub async fn delete_lark_credential(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("{}/credentials/{}", BASE, id)).await
}

/// 设置默认飞书凭证（lark_cli 工具身份优先取引用该凭证的渠道；空串取消默认）
pub async fn set_default_lark_credential(
    credential_id: &str,
) -> Result<SetDefaultLarkCredentialResponse, ApiError> {
    api_post(
        &format!("{}/credentials/default", BASE),
        &SetDefaultLarkCredentialRequest {
            credential_id: credential_id.to_string(),
        },
    )
    .await
}

// ===== 用户 OAuth device flow =====

/// 发起 device flow 授权（返回设备码 + 浏览器验证 URL）
pub async fn lark_auth_start(req: LarkAuthStartRequest) -> Result<LarkAuthStartResponse, ApiError> {
    api_post(&format!("{}/auth/start", BASE), &req).await
}

/// 完成 device flow 授权（后端轮询 device code 直到用户完成）
pub async fn lark_auth_complete(
    req: LarkAuthCompleteRequest,
) -> Result<LarkAuthCompleteResponse, ApiError> {
    api_post(&format!("{}/auth/complete", BASE), &req).await
}

/// 查询用户授权状态
#[allow(dead_code)]
pub async fn lark_auth_status() -> Result<LarkAuthStatusResponse, ApiError> {
    api_get(&format!("{}/auth/status", BASE)).await
}

/// 取消用户授权（清本机登录态）
pub async fn lark_auth_logout() -> Result<LarkAuthLogoutResponse, ApiError> {
    api_post(&format!("{}/auth/logout", BASE), &serde_json::json!({})).await
}

// ===== config init --new 自动绑定 =====

/// 发起自动绑定会话（返回会话 ID + 验证 URL）
pub async fn lark_bind_start() -> Result<LarkBindStartResponse, ApiError> {
    api_post(&format!("{}/bind/start", BASE), &serde_json::json!({})).await
}

/// 轮询绑定会话状态
pub async fn lark_bind_status(session_id: &str) -> Result<LarkBindStatusResponse, ApiError> {
    api_get(&format!("{}/bind/status?session_id={}", BASE, session_id)).await
}

/// 取消绑定会话
pub async fn lark_bind_cancel(session_id: &str) -> Result<LarkBindCancelResponse, ApiError> {
    api_post(
        &format!("{}/bind/cancel", BASE),
        &serde_json::json!({ "session_id": session_id }),
    )
    .await
}

// ===== 纯函数（轮询判定，可单测） =====

/// 绑定轮询判定结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindPollOutcome {
    /// 继续轮询
    Continue,
    /// 完成（分支 A 携带凭证/渠道；分支 B 引导补填）
    Done,
    /// 失败（携带提示）
    Failed(String),
}

/// 根据 bind/status 响应判定轮询走向（纯函数）
pub fn judge_bind_status(status: &str, error: Option<&str>) -> BindPollOutcome {
    match status {
        "pending" => BindPollOutcome::Continue,
        "done" => BindPollOutcome::Done,
        _ => BindPollOutcome::Failed(
            error
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "绑定流程异常终止".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_judge_bind_status_pending() {
        assert_eq!(
            judge_bind_status("pending", None),
            BindPollOutcome::Continue
        );
    }

    #[test]
    fn test_judge_bind_status_done() {
        assert_eq!(judge_bind_status("done", None), BindPollOutcome::Done);
    }

    #[test]
    fn test_judge_bind_status_failed_with_error() {
        assert_eq!(
            judge_bind_status("failed", Some("进程超时")),
            BindPollOutcome::Failed("进程超时".to_string())
        );
    }

    #[test]
    fn test_judge_bind_status_failed_blank_error_fallback() {
        assert_eq!(
            judge_bind_status("failed", Some("  ")),
            BindPollOutcome::Failed("绑定流程异常终止".to_string())
        );
    }

    #[test]
    fn test_judge_bind_status_unknown_treated_failed() {
        assert!(matches!(
            judge_bind_status("weird", None),
            BindPollOutcome::Failed(_)
        ));
    }
}
