//! 通用 WebSocket 客户端长连接管理器
//!
//! 只负责连接生命周期：建连、supervisor 指数退避重连、心跳、读循环、
//! 优雅关闭与连接状态快照。**不含任何业务语义**（不知道飞书 / 联邦）：
//! 帧的解析与处置由 `WsClientAdapter` 实现方全权解释（adapter 模式）。
//!
//! 心跳二态：
//! - 应用层心跳（如飞书 JSON ping）：adapter 实现 `heartbeat_frame()` 返回自定义文本帧
//! - 协议级心跳（默认）：`heartbeat_frame()` 返回 None，pkg 发 WS Ping 控制帧
//!
//! 典型用法：
//! ```ignore
//! let adapter = Arc::new(MyAdapter::new());
//! let state = ws::start_client(adapter).await?;
//! // ... 监控：state.conn_state_snapshot().await
//! ws::stop_client(state).await;
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use common::error::{Result, err};

/// 退避重连起始间隔
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
/// 退避重连封顶间隔
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// 心跳间隔（低于常见企业代理 60s idle timeout）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

// ==================== 适配器 trait ====================

/// 单帧处置动作（adapter 对每帧解释后给出的决定）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAction {
    /// 继续接收
    Continue,
    /// 结束本次连接（如服务端应用层 close 通知），由 supervisor 重连
    Reconnect,
}

/// WebSocket 客户端适配器：协议语义由实现方全权解释
///
/// pkg 对帧内容零假设——收到的每一帧文本都交给 `on_frame`，
/// 心跳帧内容、连接地址获取策略均由实现方决定。
#[async_trait]
pub trait WsClientAdapter: Send + Sync {
    /// 组件名（日志标识，如 "lark" / "federation"）
    fn name(&self) -> &str;

    /// 获取连接地址（每次建连/重连都会调用）
    ///
    /// 实现方可动态取端点（飞书端点接口）或静态解析（P7 多地址探测回退）。
    async fn endpoint(&self) -> Result<String>;

    /// 处理一帧文本消息
    ///
    /// 返回 `FrameAction::Reconnect` 表示应结束本次连接（supervisor 将退避重连）。
    async fn on_frame(&self, text: String) -> FrameAction;

    /// 应用层心跳帧内容；返回 None 时 pkg 发协议级 Ping 控制帧
    fn heartbeat_frame(&self) -> Option<String> {
        None
    }
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

// ==================== WsClientState ====================

/// WebSocket 客户端运行时状态
///
/// 持有 supervisor 任务句柄与关闭信号，用于优雅关闭。
pub struct WsClientState {
    /// supervisor 任务句柄（内含退避重连循环）
    supervisor_handle: JoinHandle<()>,
    /// 关闭信号（置 true 即退出）
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// 共享连接状态（监控快照读取入口）
    conn_state: Arc<RwLock<WsConnState>>,
}

impl WsClientState {
    /// 读取当前连接状态快照
    pub async fn conn_state_snapshot(&self) -> WsConnState {
        self.conn_state.read().await.clone()
    }
}

// ==================== supervisor ====================

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

/// 启动 WebSocket 客户端长连接（supervisor 模式）
///
/// 外层 supervisor 循环驱动 `run_connection_once`：
/// - 非 shutdown 退出 → 指数退避后重连（建连成功重置退避）
/// - shutdown 信号 → 立即退出
pub async fn start_client(adapter: Arc<dyn WsClientAdapter>) -> Result<WsClientState> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let conn_state = Arc::new(RwLock::new(WsConnState::new()));

    let supervisor_handle = tokio::spawn({
        let conn_state = conn_state.clone();
        async move {
            let name = adapter.name().to_string();
            let mut backoff: Option<Duration> = None;
            let mut has_connected_once = false;
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                let exit =
                    run_connection_once(adapter.clone(), conn_state.clone(), shutdown_rx.clone())
                        .await;
                let exit = match exit {
                    Ok(exit) => exit,
                    Err(e) => {
                        log_warn!("{} ws connection attempt failed: {}", name, e);
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
                log_info!("{} ws will reconnect in {:?}", name, delay);
                let mut sr = shutdown_rx.clone();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = sr.changed() => { break; }
                }
            }
            log_info!("{} ws supervisor exited", name);
        }
    });

    Ok(WsClientState {
        supervisor_handle,
        shutdown_tx,
        conn_state,
    })
}

/// 停止客户端长连接：发送关闭信号并等待 supervisor 退出
pub async fn stop_client(state: WsClientState) {
    let _ = state.shutdown_tx.send(true);
    // 等待 supervisor 退出（忽略错误：任务可能已退出）
    let _ = state.supervisor_handle.await;
    log_info!("ws client stopped");
}

// ==================== 单次连接生命周期 ====================

/// 单次连接生命周期：取端点 → 建连 → 心跳 + recv
///
/// 返回退出原因；shutdown 信号到达时立即清理退出。
async fn run_connection_once(
    adapter: Arc<dyn WsClientAdapter>,
    conn_state: Arc<RwLock<WsConnState>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<ConnExit> {
    let name = adapter.name();

    // 1. 获取连接地址（adapter 决定：动态端点 / 静态解析）
    conn_state.write().await.phase = WsConnPhase::Connecting;
    let ws_url = adapter.endpoint().await?;
    log_info!("{} ws connecting to endpoint", name);

    // 2. 建立 WebSocket 连接
    let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| err!(ThirdPartyError, "{} ws connect error: {}", name, e))?;
    {
        let mut state = conn_state.write().await;
        state.phase = WsConnPhase::Connected;
        state.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
    }
    log_info!("{} ws connected", name);

    let (write, mut read) = ws_stream.split();
    let write = Arc::new(Mutex::new(write));

    // 3. 心跳任务：adapter 提供应用层帧；否则发协议级 Ping
    let heartbeat_write = write.clone();
    let heartbeat_shutdown = shutdown_rx.clone();
    let heartbeat_frame = adapter.heartbeat_frame();
    let heartbeat_handle = tokio::spawn(async move {
        let mut shutdown_rx = heartbeat_shutdown;
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let mut w = heartbeat_write.lock().await;
                    let res = match &heartbeat_frame {
                        Some(text) => w.send(Message::Text(text.clone())).await,
                        None => w.send(Message::Ping(Vec::new())).await,
                    };
                    if let Err(e) = res {
                        log_warn!("ws heartbeat send failed: {}", e);
                        break;
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
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
                            && adapter.on_frame(text).await == FrameAction::Reconnect
                        {
                            // adapter 判定应结束本次连接（supervisor 重连）
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        log_error!("{} ws recv error: {}", name, e);
                        break;
                    }
                    None => {
                        log_info!("{} ws stream closed by server", name);
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
    log_info!("{} ws connection exited", name);
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::accept_async;

    /// 回显测试 adapter：收集收到的文本帧；端点指向测试监听地址
    struct EchoTestAdapter {
        url: String,
        received: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl WsClientAdapter for EchoTestAdapter {
        fn name(&self) -> &str {
            "test"
        }
        async fn endpoint(&self) -> Result<String> {
            Ok(self.url.clone())
        }
        async fn on_frame(&self, text: String) -> FrameAction {
            self.received.lock().await.push(text);
            FrameAction::Continue
        }
    }

    /// 回显测试 adapter 收到指定文本帧即请求重连
    struct CloseOnFrameAdapter {
        url: String,
        trigger: String,
        received: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl WsClientAdapter for CloseOnFrameAdapter {
        fn name(&self) -> &str {
            "test-close"
        }
        async fn endpoint(&self) -> Result<String> {
            Ok(self.url.clone())
        }
        async fn on_frame(&self, text: String) -> FrameAction {
            self.received.lock().await.push(text.clone());
            if text == self.trigger {
                FrameAction::Reconnect
            } else {
                FrameAction::Continue
            }
        }
    }

    /// 起一个本地 WS 测试服务端：每次 accept 后先推一条文本帧，然后保持连接
    async fn spawn_test_server(greeting: &'static str) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let Ok(ws) = accept_async(stream).await else {
                    continue;
                };
                let (mut write, mut read) = ws.split();
                // 建连即推一条问候帧
                if write
                    .send(Message::Text(greeting.to_string()))
                    .await
                    .is_err()
                {
                    continue;
                }
                // 保持连接直到对端关闭
                while let Some(Ok(msg)) = read.next().await {
                    let _ = msg;
                }
                let _ = write.close().await;
            }
        });
        (format!("ws://{}", addr), handle)
    }

    /// 正常路径：建连 → 服务端推帧投递 adapter → shutdown 优雅退出
    #[tokio::test(flavor = "multi_thread")]
    async fn client_receives_frames_and_shutdown() {
        let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
        let (url, server) = spawn_test_server("hello").await;

        let adapter = Arc::new(EchoTestAdapter {
            url: url.clone(),
            received: received.clone(),
        });
        let state = start_client(adapter).await.unwrap();

        // 等待建连成功
        for _ in 0..50 {
            let snap = state.conn_state_snapshot().await;
            if snap.phase == WsConnPhase::Connected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let snap = state.conn_state_snapshot().await;
        assert_eq!(snap.phase, WsConnPhase::Connected);
        assert_eq!(snap.reconnect_count, 0);
        assert!(snap.last_connected_at.is_some());

        // 服务端推的 "hello" 应已投递到 adapter
        for _ in 0..50 {
            if !received.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(*received.lock().await, vec!["hello".to_string()]);

        stop_client(state).await;
        server.abort();
    }

    /// on_frame 返回 Reconnect → 结束本次连接并自动重连（reconnect_count 增长）
    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_on_frame_action() {
        let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
        let (url, server) = spawn_test_server("bye").await;

        let adapter = Arc::new(CloseOnFrameAdapter {
            url: url.clone(),
            trigger: "bye".to_string(),
            received: received.clone(),
        });
        let state = start_client(adapter).await.unwrap();
        // 每次建连服务端都推 "bye" → 触发 Reconnect → supervisor 自动重连。
        // 首次建连不计入 reconnect_count，需等待完整退避（约 2s）+ 第二次建连完成。
        tokio::time::sleep(Duration::from_secs(4)).await;
        let snap = state.conn_state_snapshot().await;
        assert!(
            snap.reconnect_count >= 1,
            "expected at least one reconnect, snapshot: {:?}",
            snap
        );
        assert!(!received.lock().await.is_empty());

        stop_client(state).await;
        server.abort();
    }

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
