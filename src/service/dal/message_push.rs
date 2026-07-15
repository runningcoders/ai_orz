//! 消息推送 DAL
//!
//! 负责消息加工和 SSE 推送

use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use common::error::Result;
use tokio::sync::broadcast;
use crate::pkg::RequestContext;
use crate::service::dao::message_push::SsePushDao;

/// SSE 推送消息 payload，与 MessageListItem 结构对齐
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SsePushPayload {
    pub message_id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub from_id: String,
    pub from_role: i32,
    pub to_id: String,
    pub to_role: i32,
    pub message_type: i32,
    pub status: i32,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub created_at: i64,
    /// 文件类型（附件消息才有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<i32>,
    /// 文件元数据（附件消息才有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_meta: Option<common::api::message::FileMetaInfo>,
}

#[derive(Debug, Clone)]
pub struct PushResult {
    pub delivered_count: usize,
}

#[async_trait]
pub trait MessagePushDal: Send + Sync {
    async fn push_to_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        payload: &SsePushPayload,
    ) -> Result<PushResult>;

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String>;

    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str);

    async fn sse_connection_count(&self, ctx: RequestContext, user_id: &str) -> usize;
}

pub struct MessagePushDalImpl {
    sse_push_dao: Arc<dyn SsePushDao>,
}

impl MessagePushDalImpl {
    pub fn new(sse_push_dao: Arc<dyn SsePushDao>) -> Self {
        Self { sse_push_dao }
    }
}

#[async_trait]
impl MessagePushDal for MessagePushDalImpl {
    async fn push_to_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        payload: &SsePushPayload,
    ) -> Result<PushResult> {
        let json = serde_json::to_string(payload)?;
        let result = self.sse_push_dao.push(ctx, user_id, &json).await?;
        Ok(PushResult {
            delivered_count: result.delivered_count,
        })
    }

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<String> {
        self.sse_push_dao.register(ctx, user_id, connection_id).await
    }

    async fn unsubscribe_sse(&self, ctx: RequestContext, connection_id: &str) {
        self.sse_push_dao.unregister(ctx, connection_id).await;
    }

    async fn sse_connection_count(&self, ctx: RequestContext, user_id: &str) -> usize {
        self.sse_push_dao.connection_count(ctx, user_id).await
    }
}

pub fn dal() -> Arc<dyn MessagePushDal> {
    static DAL: OnceLock<Arc<dyn MessagePushDal>> = OnceLock::new();
    DAL.get_or_init(|| {
        Arc::new(MessagePushDalImpl::new(crate::service::dao::message_push::dao()))
    }).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::dao::message_push::SsePushDaoImpl;

    async fn init_test_storage() {
        crate::pkg::storage::test_support::init_for_test().await;
    }

    fn new_ctx() -> RequestContext {
        RequestContext::builder().build()
    }

    #[tokio::test]
    async fn test_push_to_sse() {
        init_test_storage().await;
        let dao = Arc::new(SsePushDaoImpl::new());
        let dal = MessagePushDalImpl::new(dao);
        let ctx = new_ctx();

        let mut rx = dal.subscribe_sse(ctx.clone(), "user_1", "conn_1").await;
        let payload = SsePushPayload {
            message_id: "msg_1".to_string(),
            project_id: Some("proj_1".to_string()),
            task_id: None,
            from_id: "user_1".to_string(),
            from_role: 1,
            to_id: "agent_1".to_string(),
            to_role: 1,
            message_type: 0,
            status: 3,
            content: "hello".to_string(),
            reply_to_id: None,
            created_at: 1234567890,
            file_type: None,
            file_meta: None,
        };
        let result = dal.push_to_sse(ctx.clone(), "user_1", &payload).await.unwrap();
        assert_eq!(result.delivered_count, 1);
        let msg = rx.try_recv().unwrap();
        let parsed: SsePushPayload = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed.message_id, "msg_1");
    }
}
