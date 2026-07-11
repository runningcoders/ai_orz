//! CronTrigger 消费者
//!
//! 负责消费 Cron 触发器事件，根据触发器配置执行相应的业务逻辑。

use super::{GenericConsumer, MessageFetcher, MessageHandler};
use crate::models::event::Event;
use crate::scheduler::CronTriggerEvent;
use async_trait::async_trait;
use common::config::TopicConsumerConfig;
use common::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

/// CronTrigger 消费者单例
static CRON_TRIGGER_CONSUMER: OnceLock<Arc<CronTriggerConsumer>> = OnceLock::new();

// ==================== 类型定义 ====================

/// CronTrigger 拉取器：封装 event_queue DAO 调用
pub struct CronTriggerFetcherImpl;

/// CronTrigger 处理器：业务逻辑
pub struct CronTriggerHandlerImpl;

/// CronTrigger 消费者具体类型
pub type CronTriggerConsumer =
    GenericConsumer<CronTriggerEvent, CronTriggerFetcherImpl, CronTriggerHandlerImpl>;

// ==================== Fetcher 实现（调用 event_queue DAO） ====================

#[async_trait]
impl MessageFetcher<CronTriggerEvent> for CronTriggerFetcherImpl {
    async fn dequeue_next(&self) -> Result<Option<CronTriggerEvent>> {
        let ctx = crate::pkg::RequestContext::new(None, None);
        let dao = crate::service::dao::event_queue::cron_trigger_dao();
        match dao.dequeue_next(ctx)? {
            Some(boxed_event) => Ok(Some(*boxed_event)),
            None => Ok(None),
        }
    }

    async fn ack(&self, event_id: &str) -> Result<()> {
        let ctx = crate::pkg::RequestContext::new(None, None);
        let dao = crate::service::dao::event_queue::cron_trigger_dao();
        dao.ack(ctx, event_id)
    }

    async fn nack(&self, event_id: &str) -> Result<()> {
        let ctx = crate::pkg::RequestContext::new(None, None);
        let dao = crate::service::dao::event_queue::cron_trigger_dao();
        dao.nack(ctx, event_id)
    }
}

// ==================== Handler 实现（业务逻辑） ====================

#[derive(Debug, Serialize, Deserialize)]
struct CronTriggerPayload {
    action: String,
    #[serde(flatten)]
    extra: Value,
}

/// Agent 休息沉淀触发器 payload
#[derive(Debug, Serialize, Deserialize)]
struct AgentRestPayload {
    /// Agent ID
    agent_id: String,
    /// 每次处理的短期记忆数量上限
    settle_limit: Option<usize>,
}

#[async_trait]
impl MessageHandler<CronTriggerEvent> for CronTriggerHandlerImpl {
    async fn handle(&self, event: &CronTriggerEvent) -> Result<()> {
        sys_debug!(
            "received cron trigger event: {} (trigger_id: {}, action to be parsed)",
            event.id(),
            event.trigger_id
        );

        let payload: CronTriggerPayload = serde_json::from_str(&event.payload).map_err(|e| {
            Error::bad_request(format!(
                "invalid cron trigger payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "cron trigger fired: {} (trigger_id: {}, action: {})",
            event.trigger_name,
            event.trigger_id,
            payload.action
        );

        match payload.action.as_str() {
            "agent_rest" => {
                self.handle_agent_rest(event, &payload.extra).await?;
            }
            _ => {
                sys_warn!(
                    "unknown action '{}' for trigger {} (id: {})",
                    payload.action,
                    event.trigger_name,
                    event.trigger_id
                );
            }
        }

        Ok(())
    }
}

// ==================== 各处理者逻辑 ====================

impl CronTriggerHandlerImpl {
    /// 创建生产处理器
    pub fn new() -> Self {
        Self
    }

    /// 处理 agent_rest action：Agent 休息沉淀
    async fn handle_agent_rest(
        &self,
        event: &CronTriggerEvent,
        extra: &Value,
    ) -> Result<()> {
        let payload: AgentRestPayload = serde_json::from_value(extra.clone()).map_err(|e| {
            Error::bad_request(format!(
                "invalid agent_rest payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "agent_rest action triggered by {} (trigger_id: {}, agent_id: {})",
            event.trigger_name,
            event.trigger_id,
            payload.agent_id
        );

        let ctx = crate::pkg::RequestContext::new(None, None);
        let settle_limit = payload.settle_limit.unwrap_or(10);

        let runtime_domain = crate::service::domain::runtime::domain();
        let settled_count = runtime_domain
            .rest_and_settle(ctx, &payload.agent_id, settle_limit)
            .await?;

        sys_info!(
            "agent {} settled {} short-term memories to knowledge nodes",
            payload.agent_id,
            settled_count
        );

        Ok(())
    }
}

// ==================== 初始化与单例访问 ====================

/// 获取 CronTrigger 消费者单例
///
/// 用于监控、统计、状态检查等场景
pub fn get_consumer() -> Option<&'static CronTriggerConsumer> {
    CRON_TRIGGER_CONSUMER.get().map(|arc| &**arc)
}

/// 初始化并启动 CronTrigger 消费者
///
/// 由 consumer::init 调用
pub async fn init(config: &TopicConsumerConfig) -> Result<()> {
    sys_info!("initializing cron trigger consumer...");

    let fetcher = CronTriggerFetcherImpl;
    let handler = CronTriggerHandlerImpl::new();

    let consumer = CronTriggerConsumer::new("cron_trigger", config.clone(), fetcher, handler);

    let consumer_arc = Arc::new(consumer);
    CRON_TRIGGER_CONSUMER
        .set(consumer_arc.clone())
        .map_err(|_| {
            Error::internal("cron trigger consumer already initialized".to_string())
        })?;

    consumer_arc.start().await;

    sys_info!("cron trigger consumer started");
    Ok(())
}
