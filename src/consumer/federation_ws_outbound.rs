//! 联邦 WS 出站消费者
//!
//! 订阅 `federation.outbound`（业务 publish 的对端命令帧 / 响应帧），
//! 查连接注册表取对端出站句柄并 push。无活连接时告警丢弃——
//! 命令发起方（`call_peer` facade）应先查注册表决定走 WS 还是回退 HTTP；
//! 本 consumer 只负责「已决定走 WS」的帧投递与 event_id 幂等。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::models::events::FederationOutboundEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::dao::organization_link::ws;

pub struct FederationWsOutboundConsumer;

impl Default for FederationWsOutboundConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl FederationWsOutboundConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for FederationWsOutboundConsumer {
    fn name(&self) -> &str {
        "federation_ws_outbound"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("federation.outbound")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        // push 涉及网络 IO（写端互斥锁），不阻塞事件 worker
        ConsumeMode::Async
    }

    async fn on_event(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: FederationOutboundEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!(
                "failed to deserialize FederationOutboundEvent: {}",
                e
            ))
        })?;

        match ws::registry().lookup(&event.peer_org) {
            Some(tx) => {
                let text = serde_json::to_string(&event.frame).map_err(|e| {
                    Error::internal(format!("federation frame serialize failed: {}", e))
                })?;
                tx.send_text(text).await?;
                log_debug!(
                    "federation outbound pushed: peer={} kind={} correlation_id={}",
                    event.peer_org,
                    event.frame.kind,
                    event.frame.correlation_id
                );
            }
            None => {
                // 无活连接：告警丢弃。发起方应先查 connected 决定通道；
                // response 帧丢失由发起侧 pending 超时兜底。
                log_warn!(
                    "federation outbound dropped (no connection): peer={} kind={} correlation_id={}",
                    event.peer_org,
                    event.frame.kind,
                    event.frame.correlation_id
                );
            }
        }
        Ok(())
    }
}
