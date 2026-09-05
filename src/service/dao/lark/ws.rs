//! 飞书 WebSocket 长连接适配器
//!
//! 连接生命周期（supervisor 退避重连 / 心跳 / 读循环 / 状态快照 / 优雅关闭）
//! 由通用组件 `pkg::ws` 承载；本文件只剩飞书协议语义：
//! - 动态获取连接地址（`/open-apis/callback/ws/endpoints`）
//! - 应用层心跳帧 `{"type":"ping"}`（飞书要求，非 WS 协议 Ping）
//! - 事件帧解析：`im.message.receive_v1` → publish AOP 事件（`LarkInboundEvent`），
//!   由业务 consumer 异步消费，**读循环里不做业务**
//! - 重连时经 `WsTokenSource` 实时取 token（过期自愈）
//!
//! 协议参考：飞书长连接接收事件文档
//! https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/sdk/server-side-sdk/go--sdk/preparations-before-development/use-long-connection-receiving-events

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::error::from_reqwest;
use crate::models::events::{LarkInboundEvent, LarkMessageEvent};
use common::error::{Result, err};

use crate::pkg::RequestContext;
use crate::pkg::ws::{FrameAction, WsClientAdapter};

const PATH_WS_ENDPOINT: &str = "/open-apis/callback/ws/endpoints";

// ==================== 对外类型（生命周期部分由 pkg::ws 提供） ====================

/// 飞书 WS 客户端运行时状态（= pkg 通用客户端状态）
pub type WsState = crate::pkg::ws::WsClientState;

pub use crate::pkg::ws::{WsConnPhase, WsConnState};

/// 停止事件循环（pkg 通用关闭）
pub use crate::pkg::ws::stop_client as stop_event_loop;

// ==================== 连接地址响应 ====================

#[derive(Debug, Deserialize)]
struct WsEndpointData {
    #[serde(rename = "URL")]
    url: String,
}

#[derive(Debug, Deserialize)]
struct WsEndpointResp {
    code: i32,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<WsEndpointData>,
}

// ==================== WebSocket 消息类型 ====================

/// 飞书 WebSocket 下行消息的 type 字段
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WsIncoming {
    /// 服务端推送事件
    #[serde(rename = "event")]
    Event {
        #[serde(flatten)]
        event: Box<LarkMessageEvent>,
    },
    /// 心跳响应
    #[serde(rename = "pong")]
    Pong,
    /// 服务端关闭通知
    #[serde(rename = "close")]
    Close,
    /// 其他未识别类型（忽略）
    #[serde(other)]
    Other,
}

/// 客户端发送的心跳消息（应用层 JSON 帧）
#[derive(Debug, Serialize)]
struct WsPing {
    #[serde(rename = "type")]
    msg_type: &'static str,
}

// ==================== token 来源 ====================

/// WS 连接的 token 来源
///
/// 重连时实时取 token（带缓存刷新），避免缓存过期后死连。
#[async_trait::async_trait]
pub trait WsTokenSource: Send + Sync {
    /// 获取当前可用的 tenant_access_token
    async fn token(&self) -> Result<String>;
}

// ==================== 飞书协议适配器 ====================

/// 获取连接地址（token 经 `WsTokenSource` 实时获取）
async fn fetch_ws_endpoint(
    http: &reqwest::Client,
    token_source: &Arc<dyn WsTokenSource>,
) -> Result<String> {
    let token = token_source.token().await?;

    let url = format!("https://open.feishu.cn{}", PATH_WS_ENDPOINT);
    let resp = http
        .post(&url)
        .bearer_auth(&token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|e| from_reqwest("ws_endpoint", e))?
        .json::<WsEndpointResp>()
        .await
        .map_err(|e| from_reqwest("ws_endpoint", e))?;

    if resp.code != 0 {
        return Err(err!(
            ThirdPartyError,
            "lark ws_endpoint failed: code={} msg={}",
            resp.code,
            resp.msg
        ));
    }
    let data = resp
        .data
        .ok_or_else(|| err!(ThirdPartyError, "lark ws_endpoint returned empty data"))?;
    if data.url.is_empty() {
        return Err(err!(ThirdPartyError, "lark ws_endpoint returned empty URL"));
    }
    Ok(data.url)
}

/// 飞书 WS 适配器：实现 pkg 通用客户端 trait，只含飞书协议语义
struct LarkWsAdapter {
    http: reqwest::Client,
    app_id: String,
    token_source: Arc<dyn WsTokenSource>,
}

#[async_trait::async_trait]
impl WsClientAdapter for LarkWsAdapter {
    fn name(&self) -> &str {
        "lark"
    }

    /// 每次建连/重连动态取飞书端点
    async fn endpoint(&self) -> Result<String> {
        fetch_ws_endpoint(&self.http, &self.token_source).await
    }

    /// 处理一帧飞书消息；服务端 close 通知 → Reconnect（supervisor 重连）
    async fn on_frame(&self, text: String) -> FrameAction {
        let incoming: WsIncoming = match serde_json::from_str(&text) {
            Ok(i) => i,
            Err(e) => {
                log_debug!(
                    "lark ws parse message failed (ignored): {} body={}",
                    e,
                    text
                );
                return FrameAction::Continue;
            }
        };

        match incoming {
            WsIncoming::Event { event } => {
                let event_id = event.header.event_id.clone();
                let event_type = event.header.event_type.clone();
                log_info!(
                    "lark ws received event: id={} type={}",
                    event_id,
                    event_type
                );

                // 仅处理 im.message.receive_v1 事件
                if event_type != "im.message.receive_v1" {
                    log_debug!("lark ws skip non-message event: type={}", event_type);
                    return FrameAction::Continue;
                }

                // publish AOP 事件（入队即返回），业务由 consumer 异步消费——读循环不做业务
                let aop_event = LarkInboundEvent {
                    app_id: self.app_id.clone(),
                    event: *event,
                };
                let ctx = RequestContext::new_system();
                crate::pkg::aop::registry().publish(&ctx, aop_event).await;
                FrameAction::Continue
            }
            WsIncoming::Pong => {
                // 心跳响应，无需处理
                FrameAction::Continue
            }
            WsIncoming::Close => {
                log_info!("lark ws received close from server");
                FrameAction::Reconnect
            }
            WsIncoming::Other => {
                // 忽略未识别消息
                FrameAction::Continue
            }
        }
    }

    /// 飞书要求应用层 JSON ping（非 WS 协议 Ping 控制帧）
    fn heartbeat_frame(&self) -> Option<String> {
        Some(
            serde_json::to_string(&WsPing { msg_type: "ping" })
                .unwrap_or_else(|_| r#"{"type":"ping"}"#.to_string()),
        )
    }
}

// ==================== 启动入口 ====================

/// 启动 WebSocket 事件循环（supervisor 模式，由 pkg::ws 驱动）
///
/// - 非 shutdown 退出 → 指数退避重连（1s 起、倍增至 60s 封顶、±20% 抖动）
/// - shutdown 信号 → 立即退出
pub async fn start_event_loop(
    http: reqwest::Client,
    app_id: String,
    token_source: Arc<dyn WsTokenSource>,
) -> Result<WsState> {
    let adapter = Arc::new(LarkWsAdapter {
        http,
        app_id,
        token_source,
    });
    crate::pkg::ws::start_client(adapter).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 退避逻辑已迁至 pkg::ws（next_backoff + WsConnPhase 测试随迁），
    /// 此处仅确认重导出可用（监控快照消费方依赖这些类型稳定存在）。
    #[test]
    fn reexports_are_stable() {
        assert_eq!(WsConnPhase::Connecting.as_str(), "connecting");
        assert_eq!(WsConnPhase::Connected.as_str(), "connected");
        assert_eq!(WsConnPhase::Reconnecting.as_str(), "reconnecting");
    }

    /// 心跳帧是飞书应用层 JSON ping
    #[test]
    fn heartbeat_frame_is_app_layer_ping() {
        let adapter = LarkWsAdapter {
            http: reqwest::Client::new(),
            app_id: "app".to_string(),
            token_source: Arc::new(NoopTokenSource),
        };
        let frame = WsClientAdapter::heartbeat_frame(&adapter).expect("lark uses app-layer ping");
        assert_eq!(frame, r#"{"type":"ping"}"#);
    }

    struct NoopTokenSource;
    #[async_trait::async_trait]
    impl WsTokenSource for NoopTokenSource {
        async fn token(&self) -> Result<String> {
            Ok("token".to_string())
        }
    }
}
