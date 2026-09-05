//! 联邦入站命令消费者：A2A 委派（P8 最小闭环唯一命令）
//!
//! 订阅 `federation.inbound.send_task`（对端命令帧经 session publish），
//! 复用 HTTP 路径的核心函数 `handle_send_task(ctx, params)`——domain/DAL
//! 零改动；执行完 publish `FederationOutboundEvent`（response 帧），
//! 由出站 consumer 推回对端，与发起侧 pending 表配对。
//!
//! **Async 模式**：publish 发生在 WS 读循环，Sync 会阻塞整条连接。

use async_trait::async_trait;
use common::api::a2a::SendTaskParams;
use common::enums::CallerType;
use common::error::{Error, Result};
use serde_json::json;

use crate::handlers::a2a::send_task::handle_send_task;
use crate::models::events::{FederationFrame, FederationInboundEvent, FederationOutboundEvent};
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::organization;

pub struct FederationInboundTaskConsumer;

impl Default for FederationInboundTaskConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl FederationInboundTaskConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for FederationInboundTaskConsumer {
    fn name(&self) -> &str {
        "federation_inbound_task"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![FederationInboundEvent::KIND_SEND_TASK]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // 读循环里不做业务：A2A 委派可能耗时（project/agent/message 全链路）
        ConsumeMode::Async
    }

    async fn on_event(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: FederationInboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize FederationInboundEvent: {}",
                e
            ))
        })?;

        // 构造业务 ctx（P6 接待模型）：**本端**接待用户 + 对端 caller_org，
        // 按事件解析（事件级注入两端 org ID，帧信封零身份）；
        // reception_user 为单条查询，相对 A2A 任务全链路开销可忽略
        let reception = organization::domain()
            .user_manage()
            .reception_user(RequestContext::new_system(), &event.local_org)
            .await
            .map_err(|e| {
                log_error!(
                    "federation inbound rejected: peer={} correlation_id={} err={}",
                    event.peer_org,
                    event.frame.correlation_id,
                    e
                );
                e
            })?;
        let task_ctx = RequestContext::builder()
            .caller_type(CallerType::User)
            .user_id(reception.id.clone())
            .username(format!("federation:{}", event.peer_org))
            .organization_id(reception.organization_id.clone())
            .try_caller_organization_id(Some(event.peer_org.clone()))
            .build();

        // 反序列化命令参数；失败直接回错误响应（对端 pending 立刻失败，不悬挂）
        let params: SendTaskParams = match serde_json::from_value(event.frame.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                self.reply_error(
                    &event.peer_org,
                    &event.frame.correlation_id,
                    format!("invalid send_task params: {}", e),
                )
                .await;
                return Ok(());
            }
        };

        // 调 HTTP 路径同一核心函数（ctx 带接待用户 + caller_org，来源审计 tag 生效）
        let reply = match handle_send_task(task_ctx, params).await {
            Ok(task) => match serde_json::to_value(&task) {
                Ok(v) => json!({"ok": true, "task": v}),
                Err(e) => json!({"ok": false, "error": format!("serialize task failed: {}", e)}),
            },
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };

        // 回推响应帧（经出站事件 → WS 出站 consumer）
        let outbound = FederationOutboundEvent {
            peer_org: event.peer_org.clone(),
            frame: FederationFrame::response(event.frame.correlation_id.clone(), reply),
        };
        aop_publish_outbound(outbound).await;
        Ok(())
    }
}

impl FederationInboundTaskConsumer {
    async fn reply_error(&self, peer_org: &str, correlation_id: &str, message: String) {
        let outbound = FederationOutboundEvent {
            peer_org: peer_org.to_string(),
            frame: FederationFrame::response(
                correlation_id.to_string(),
                json!({"ok": false, "error": message}),
            ),
        };
        aop_publish_outbound(outbound).await;
    }
}

async fn aop_publish_outbound(event: FederationOutboundEvent) {
    let ctx = RequestContext::new_system();
    crate::pkg::aop::registry().publish(&ctx, event).await;
}
