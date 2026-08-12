//! 飞书 WebSocket 长连接实现
//!
//! 负责：
//! - 获取飞书 WebSocket 连接地址（`/open-apis/callback/ws/endpoints`）
//! - 建立 WebSocket 连接
//! - 维持心跳（飞书要求客户端定期发送 ping）
//! - 接收事件消息并回调 `LarkEventHandler`
//! - **supervisor 退避重连**：连接断开后指数退避重连（1s 起、倍增至 60s 封顶、±20% 抖动），
//!   重连时经 `WsTokenSource` 实时取 token（过期自愈）
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
use super::error::{from_reqwest, from_ws};
use super::event::LarkMessageEvent;
use common::error::{Result, err};

const PATH_WS_ENDPOINT: &str = "/open-apis/callback/ws/endpoints";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 退避重连起始间隔
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
/// 退避重连封顶间隔
const BACKOFF_MAX: Duration = Duration::from_secs(60);

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

// ==================== token 来源 ====================

/// WS 连接的 token 来源
///
/// 重连时实时取 token（带缓存刷新），避免缓存过期后死连。
#[async_trait::async_trait]
pub trait WsTokenSource: Send + Sync {
    /// 获取当前可用的 tenant_access_token
    async fn token(&self) -> Result<String>;
}

// ==================== 连接状态监控 ====================

/// WS 连接所处阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsConnPhase {
    /// 正在建立连接（首次）
    Connecting,
    /// 已连接
    Connected,
    /// 断线后退避重连中
    Reconnecting,
}

impl WsConnPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            WsConnPhase::Connecting => "connecting",
            WsConnPhase::Connected => "connected",
            WsConnPhase::Reconnecting => "reconnecting",
        }
    }
}

/// WS 连接运行时状态快照（监控用）
#[derive(Debug, Clone)]
pub struct WsConnState {
    pub phase: WsConnPhase,
    /// 累计重连成功次数（首次建连不计）
    pub reconnect_count: u64,
    /// 最近一次建连成功时间（RFC3339）
    pub last_connected_at: Option<String>,
}

impl WsConnState {
    fn new() -> Self {
        Self {
            phase: WsConnPhase::Connecting,
            reconnect_count: 0,
            last_connected_at: None,
        }
    }
}

// ==================== WsState ====================

/// WebSocket 连接运行时状态
///
/// 持有 supervisor 任务句柄与关闭信号，用于优雅关闭。
pub struct WsState {
    /// supervisor 任务句柄（内含退避重连循环）
    supervisor_handle: JoinHandle<()>,
    /// 关闭信号（置 true 即退出）
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// 共享连接状态（监控快照读取入口）
    conn_state: Arc<RwLock<WsConnState>>,
}

impl WsState {
    /// 读取当前连接状态快照
    pub async fn conn_state_snapshot(&self) -> WsConnState {
        self.conn_state.read().await.clone()
    }
}

/// 单次连接的退出原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnExit {
    /// 是否因 shutdown 信号退出（true 则 supervisor 终止，不重连）
    shutdown: bool,
    /// 本次连接是否曾成功建连（用于重置退避）
    connected: bool,
}

/// 计算下一次退避间隔（纯函数，可测）
///
/// 规则：当前间隔倍增（无当前值则从 1s 起），封顶 60s，叠加 ±20% 抖动后再次封顶。
pub fn next_backoff(current: Option<Duration>) -> Duration {
    use rand::Rng;
    let base = current.unwrap_or(BACKOFF_INITIAL);
    let doubled = base.saturating_mul(2).min(BACKOFF_MAX);
    let jitter = rand::thread_rng().gen_range(0.8..=1.2);
    doubled.mul_f64(jitter).min(BACKOFF_MAX)
}

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

/// 启动 WebSocket 事件循环（supervisor 模式）
///
/// 外层 supervisor 循环驱动 `run_connection_once`：
/// - 非 shutdown 退出 → 指数退避后重连（建连成功重置退避）
/// - shutdown 信号 → 立即退出
pub async fn start_event_loop(
    http: reqwest::Client,
    app_id: String,
    token_source: Arc<dyn WsTokenSource>,
    handler: Arc<dyn LarkEventHandler>,
) -> Result<WsState> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let conn_state = Arc::new(RwLock::new(WsConnState::new()));

    let supervisor_handle = tokio::spawn({
        let conn_state = conn_state.clone();
        async move {
            let mut backoff: Option<Duration> = None;
            let mut has_connected_once = false;
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                let exit = run_connection_once(
                    http.clone(),
                    app_id.clone(),
                    token_source.clone(),
                    handler.clone(),
                    conn_state.clone(),
                    shutdown_rx.clone(),
                )
                .await;
                let exit = match exit {
                    Ok(exit) => exit,
                    Err(e) => {
                        log_warn!("lark ws connection attempt failed app_id={}: {}", app_id, e);
                        ConnExit {
                            shutdown: false,
                            connected: false,
                        }
                    }
                };
                if exit.shutdown {
                    break;
                }
                if exit.connected {
                    // 建连成功 → 重置退避；非首次视为一次重连成功
                    backoff = None;
                    if has_connected_once {
                        conn_state.write().await.reconnect_count += 1;
                    }
                    has_connected_once = true;
                }
                let delay = next_backoff(backoff);
                backoff = Some(delay);
                conn_state.write().await.phase = WsConnPhase::Reconnecting;
                log_info!("lark ws will reconnect app_id={} in {:?}", app_id, delay);
                let mut sr = shutdown_rx.clone();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = sr.changed() => { break; }
                }
            }
            log_info!("lark ws supervisor exited app_id={}", app_id);
        }
    });

    Ok(WsState {
        supervisor_handle,
        shutdown_tx,
        conn_state,
    })
}

/// 单次连接生命周期：取端点 → 建连 → 心跳 + recv
///
/// 返回退出原因；shutdown 信号到达时立即清理退出。
async fn run_connection_once(
    http: reqwest::Client,
    app_id: String,
    token_source: Arc<dyn WsTokenSource>,
    handler: Arc<dyn LarkEventHandler>,
    conn_state: Arc<RwLock<WsConnState>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<ConnExit> {
    // 1. 获取连接地址
    conn_state.write().await.phase = WsConnPhase::Connecting;
    let ws_url = fetch_ws_endpoint(&http, &token_source).await?;
    log_info!("lark ws connecting to endpoint for app_id={}", app_id);

    // 2. 建立 WebSocket 连接
    let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| from_ws("connect", e))?;
    {
        let mut state = conn_state.write().await;
        state.phase = WsConnPhase::Connected;
        state.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
    }
    log_info!("lark ws connected app_id={}", app_id);

    let (write, mut read) = ws_stream.split();
    let write = Arc::new(Mutex::new(write));

    // 3. 心跳任务
    let heartbeat_write = write.clone();
    let heartbeat_shutdown = shutdown_rx.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut shutdown_rx = heartbeat_shutdown;
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
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        log_info!("lark ws heartbeat task shutting down");
                        break;
                    }
                }
            }
        }
    });

    // 4. 接收循环
    let mut exit = ConnExit {
        shutdown: false,
        connected: true,
    };
    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    exit.shutdown = true;
                    break;
                }
            }
            frame = read.next() => {
                match frame {
                    Some(Ok(msg)) => {
                        if let Message::Text(text) = msg
                            && !handle_ws_message(&text, &app_id, &handler).await
                        {
                            // 服务端关闭通知 → 退出本次连接（supervisor 重连）
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        log_error!("lark ws recv error: {}", e);
                        break;
                    }
                    None => {
                        log_info!("lark ws stream closed by server");
                        break;
                    }
                }
            }
        }
    }

    // 关闭 write 端并等待心跳任务退出
    {
        let mut w = write.lock().await;
        let _ = w.close().await;
    }
    heartbeat_handle.abort();
    let _ = heartbeat_handle.await;
    log_info!("lark ws connection exited app_id={}", app_id);
    Ok(exit)
}

/// 处理收到的 WebSocket 文本消息
///
/// 返回 `true` 表示继续接收，`false` 表示应结束本次连接（服务端 close）。
async fn handle_ws_message(text: &str, app_id: &str, handler: &Arc<dyn LarkEventHandler>) -> bool {
    let incoming: WsIncoming = match serde_json::from_str(text) {
        Ok(i) => i,
        Err(e) => {
            log_debug!(
                "lark ws parse message failed (ignored): {} body={}",
                e,
                text
            );
            return true;
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
                return true;
            }

            // 回调 handler（错误仅记录，不中断循环）
            if let Err(e) = handler.handle_message_event(app_id, *event).await {
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
            return false;
        }
        WsIncoming::Other => {
            // 忽略未识别消息
        }
    }
    true
}

/// 停止事件循环
///
/// 发送关闭信号并等待 supervisor 退出。
pub async fn stop_event_loop(state: WsState) {
    let _ = state.shutdown_tx.send(true);
    // 等待 supervisor 退出（忽略错误：任务可能已退出）
    let _ = state.supervisor_handle.await;
    log_info!("lark ws event loop stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 退避序列：从 1s 起倍增，封顶 60s，±20% 抖动范围内
    #[test]
    fn next_backoff_grows_and_caps() {
        // 首次退避基准 2s（1s 倍增），允许 ±20% 抖动
        for _ in 0..20 {
            let d = next_backoff(None);
            assert!(d >= Duration::from_millis(1600) && d <= Duration::from_millis(2400));
        }
        // 连续倍增不超过封顶（含抖动后二次封顶）
        let mut current = Some(Duration::from_secs(1));
        for _ in 0..10 {
            current = Some(next_backoff(current));
            assert!(current.unwrap() <= BACKOFF_MAX);
        }
        // 已达封顶后仍维持在封顶以内
        let d = next_backoff(Some(BACKOFF_MAX));
        assert!(d >= Duration::from_secs(48) && d <= BACKOFF_MAX);
    }

    /// WsConnPhase 字符串表示稳定（监控快照消费）
    #[test]
    fn ws_conn_phase_as_str() {
        assert_eq!(WsConnPhase::Connecting.as_str(), "connecting");
        assert_eq!(WsConnPhase::Connected.as_str(), "connected");
        assert_eq!(WsConnPhase::Reconnecting.as_str(), "reconnecting");
    }
}
