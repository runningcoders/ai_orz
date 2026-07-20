//! A2A Callback 渠道 DAO HTTP 实现

use super::A2aCallbackDao;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::service::domain::message;
use crate::service::domain::project::domain as project_domain;
use common::error::err;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static A2A_CALLBACK_DAO: OnceLock<Arc<dyn A2aCallbackDao>> = OnceLock::new();

/// 创建一个全新的 A2A Callback DAO 实例（用于测试）
pub fn new() -> Arc<dyn A2aCallbackDao> {
    Arc::new(A2aCallbackDaoHttpImpl::new())
}

/// 获取 A2aCallbackDao 单例
pub fn dao() -> Arc<dyn A2aCallbackDao> {
    A2A_CALLBACK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = A2A_CALLBACK_DAO.set(new());
}

// ==================== 实现 ====================

struct A2aCallbackDaoHttpImpl;

impl A2aCallbackDaoHttpImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl A2aCallbackDao for A2aCallbackDaoHttpImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        let webhook_url = channel.po.webhook_url.as_ref()
            .ok_or_else(|| err!(InvalidRequest, "A2A callback 渠道缺少 webhook_url"))?;

        let project_id = channel.po.scope_project.as_ref()
            .or_else(|| message.po.project_id.as_ref())
            .ok_or_else(|| err!(InvalidRequest, "A2A callback 渠道缺少 scope_project"))?;

        let project = project_domain()
            .project_manage()
            .get(ctx.clone(), project_id)
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "项目不存在: {}", project_id))?;

        let messages = message::domain()
            .management()
            .list_by_project_id(ctx.clone(), project_id)
            .await?;

        let a2a_messages = messages
            .into_iter()
            .map(|msg| {
                let role = match msg.po.from_role {
                    common::enums::MessageRole::User => "user".to_string(),
                    common::enums::MessageRole::Agent => "agent".to_string(),
                    common::enums::MessageRole::System => "system".to_string(),
                };
                common::api::a2a::A2aMessage {
                    role,
                    parts: vec![common::api::a2a::A2aMessagePart::Text {
                        text: msg.po.content.clone(),
                    }],
                    message_id: Some(msg.po.id.clone()),
                    task_id: Some(project_id.to_string()),
                }
            })
            .collect::<Vec<_>>();

        let task = common::api::a2a::A2aTask {
            id: project_id.to_string(),
            session_id: None,
            status: common::api::a2a::A2aTaskStatus {
                state: match project.po.status {
                    common::enums::ProjectStatus::PendingReview => common::api::a2a::A2aTaskState::Submitted,
                    common::enums::ProjectStatus::InProgress => common::api::a2a::A2aTaskState::Working,
                    common::enums::ProjectStatus::Completed => common::api::a2a::A2aTaskState::Completed,
                    common::enums::ProjectStatus::Archived => common::api::a2a::A2aTaskState::Completed,
                    common::enums::ProjectStatus::Deleted => common::api::a2a::A2aTaskState::Canceled,
                    _ => common::api::a2a::A2aTaskState::Working,
                },
                timestamp: chrono::Utc::now().to_rfc3339(),
                message: None,
            },
            messages: a2a_messages,
            artifacts: vec![],
            metadata: serde_json::Value::Object(Default::default()),
        };

        let body = serde_json::to_string(&task)
            .map_err(|e| err!(Internal, "序列化 A2A Task 失败: {}", e))?;

        let client = reqwest::Client::new();
        let resp = client
            .post(webhook_url)
            .header("Content-Type", "application/json")
            .body(body)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| err!(ChannelPushFailed, "A2A callback 请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(err!(ChannelPushFailed, "A2A callback 返回错误状态码 {}: {}", status, body));
        }

        Ok(())
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        channel: &MessageChannel,
    ) -> std::result::Result<(), common::error::Error> {
        let msg_po = crate::models::message::MessagePo::default();
        let msg = crate::models::message::Message::from_po(msg_po);
        self.push(ctx, &msg, channel).await
    }
}
