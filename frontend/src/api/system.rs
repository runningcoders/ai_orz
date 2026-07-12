//! System 域 API - 健康检查、定时触发器

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put};

/// 健康检查（返回纯文本）
pub async fn check_health() -> Result<String, String> {
    super::api_get_text("/health").await
}

// ===== 定时触发器 =====

pub async fn list_cron_triggers() -> Result<common::api::ListCronTriggersResponse, String> {
    api_get_or_default("/api/v1/system/cron-triggers").await
}

pub async fn get_cron_trigger(id: &str) -> Result<common::api::GetCronTriggerResponse, String> {
    api_get(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn create_cron_trigger(req: common::api::CreateCronTriggerRequest) -> Result<common::api::CreateCronTriggerResponse, String> {
    api_post("/api/v1/system/cron-triggers", &req).await
}

pub async fn update_cron_trigger(id: &str, req: common::api::UpdateCronTriggerRequest) -> Result<common::api::UpdateCronTriggerResponse, String> {
    api_put(&format!("/api/v1/system/cron-triggers/{}", id), &req).await
}

pub async fn delete_cron_trigger(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/system/cron-triggers/{}", id)).await
}

pub async fn pause_cron_trigger(id: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/pause", id), &body).await
}

pub async fn resume_cron_trigger(id: &str) -> Result<(), String> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/system/cron-triggers/{}/resume", id), &body).await
}
