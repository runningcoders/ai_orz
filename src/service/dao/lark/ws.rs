//! 飞书 WebSocket 长连接实现
//!
//! 负责：
//! - 获取飞书 WebSocket 连接地址（`/open-apis/callback/ws/endpoints`）
//! - 建立 WebSocket 连接
//! - 维持心跳（飞书要求客户端定期发送 ping）
//! - 接收事件消息并回调 `LarkEventHandler`
//!
//! 协议参考：飞书长连接接收事件文档
//! https://open.feishu.cn/document/uAjLw4CM/ukTMukTMukTM/reference/sdk/server-side-sdk/go--sdk/preparations-before-development/use-long-connection-receiving-events

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use super::LarkEventHandler;
use super::error::{from_reqwest, from_serde, from_ws, validate_config};
use super::event::LarkMessageEvent;
use super::token::SharedTokenCache;
use common::config::LarkConfig;
use common::error::{Result, err};

const PATH_WS_ENDPOINT: &str = "/open-apis/callback/ws/endpoints";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

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

/// 客户端发送的心跳消息
#[derive(Debug, Serialize)]
struct WsPing {
    #[serde(rename = "type")]
    msg_type: &'static str,
}

// ==================== WsState ====================

/// WebSocket 连接运行时状态
///
/// 用于管理后台任务和优雅关闭。
pub struct WsState {
    /// 心 beat 任务句柄
    heartbeat_handle: JoinHandle<()>,
    /// 消息接收任务句柄
    recv_handle: JoinHandle<()>,
    /// 关闭信号
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

/// 获取连接地址
async fn fetch_ws_endpoint(
    http: &reqwest::Client,
    config: &LarkConfig,
    token_cache: SharedTokenCache,
) -> Result<String> {
    validate_config(&config.app_id, &config.app_secret)?;

    // 复用 http.rs 中的 token 获取逻辑（通过 dao 单例）
    // 这里直接从 token_cache 读取，如果无效则由外部刷新
    let token = {
        let cache = token_cache.read().await;
        cache.get_valid_token()
    };
    let token = match token {
        Some(t) => t,
        None => {
            return Err(err!(
                ThirdPartyError,
                "lark ws endpoint fetch failed: no valid tenant_access_token"
            ));
        }
    };

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

/// 启动 WebSocket 事件循环
///
/// 流程：
/// 1. 获取连接地址
/// 2. 建立 WebSocket 连接
/// 3. 启动心跳任务（30s 发送一次 ping）
/// 4. 启动接收任务（解析事件并回调 handler）
pub async fn start_event_loop(
    http: reqwest::Client,
    config: LarkConfig,
    token_cache: SharedTokenCache,
    handler: Arc<dyn LarkEventHandler>,
) -> Result<WsState> {
    // 1. 获取连接地址
    let ws_url = fetch_ws_endpoint(&http, &config, token_cache).await?;
    log_info!("lark ws connecting to endpoint");

    // 2. 建立 WebSocket 连接
    let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| from_ws("connect", e))?;
    log_info!("lark ws connected");

    let (write, mut read) = ws_stream.split();
    let write = Arc::new(Mutex::new(write));

    // 3. 关闭信号
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // 4. 心跳任务
    let heartbeat_write = write.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let ping = serde_json::to_string(&WsPing { msg_type: "ping" })
                        .unwrap_or_else(|_| r#"{"type":"ping"}"#.to_string());
                    let mut w = heartbeat_write.lock().await;
                    if let Err(e) = w.send(Message::Text(ping)).await {
                        log_warn!("lark ws heartbeat send failed: {}", e);
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    log_info!("lark ws heartbeat task shutting down");
                    break;
                }
            }
        }
    });

    // 5. 接收任务
    let recv_write = write.clone();
    let recv_handle = tokio::spawn(async move {
        loop {
            match read.next().await {
                Some(Ok(msg)) => {
                    if let Message::Text(text) = msg {
                        handle_ws_message(&text, &handler).await;
                    }
                }
                Some(Err(e)) => {
                    log_error!("lark ws recv error: {}", e);
                    // 通知 write 端关闭
                    let mut w = recv_write.lock().await;
                    let _ = w.close().await;
                    break;
                }
                None => {
                    log_info!("lark ws stream closed by server");
                    break;
                }
            }
        }
        log_info!("lark ws recv task exited");
    });

    Ok(WsState {
        heartbeat_handle,
        recv_handle,
        shutdown_tx,
    })
}

/// 处理收到的 WebSocket 文本消息
async fn handle_ws_message(text: &str, handler: &Arc<dyn LarkEventHandler>) {
    let incoming: WsIncoming = match serde_json::from_str(text) {
        Ok(i) => i,
        Err(e) => {
            log_debug!(
                "lark ws parse message failed (ignored): {} body={}",
                e,
                text
            );
            return;
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
                return;
            }

            // 回调 handler（错误仅记录，不中断循环）
            if let Err(e) = handler.handle_message_event(*event).await {
                log_error!(
                    "lark ws event handler error: event_id={} err={}",
                    event_id,
                    e
                );
            }
        }
        WsIncoming::Pong => {
            // 心跳响应，无需处理
        }
        WsIncoming::Close => {
            log_info!("lark ws received close from server");
        }
        WsIncoming::Other => {
            // 忽略未识别消息
        }
    }
}

/// 停止事件循环
///
/// 发送关闭信号并等待任务退出。
pub async fn stop_event_loop(state: WsState) {
    let _ = state.shutdown_tx.send(());
    // 等待任务退出（忽略错误：任务可能已退出）
    let _ = state.heartbeat_handle.await;
    let _ = state.recv_handle.await;
    log_info!("lark ws event loop stopped");
}

// 抑制未使用导入告警
#[allow(dead_code)]
fn _ensure_imports_used() {
    let _ = from_serde;
    let _ = RwLock::<()>::new(());
}
