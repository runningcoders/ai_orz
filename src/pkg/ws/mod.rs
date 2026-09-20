//! 通用 WebSocket 客户端长连接管理器
//!
//! 只负责连接生命周期：建连、重连（指数退避 / 固定间隔可配）、心跳、读循环、
//! 优雅关闭与连接状态快照。**不含任何业务语义**（不知道飞书 / 联邦）：
//! 帧的解析与处置由 `WsClientAdapter` 实现方全权解释（adapter 模式）。
//!
//! # 帧形态
//!
//! - 文本帧：`on_frame(String)`（既有通路，联邦使用）
//! - 二进制帧：`on_message(WsFrame)`（新通路，飞书 pbbp2 protobuf 使用）
//!
//! `on_message` 的默认实现把文本转投 `on_frame`、对二进制仅**留痕不静默**，
//! 因此既有实现方（联邦）无需任何改动。
//!
//! # 心跳
//!
//! - 应用层心跳（如飞书 JSON ping）：adapter 实现 `heartbeat_frame()` 返回自定义文本帧
//! - 协议级心跳（默认）：`heartbeat_frame()` 返回 None，pkg 发 WS Ping 控制帧
//! - 心跳**间隔与帧内容**由 `heartbeat()` **每 tick 重新查询**（服务端可在运行期下发新值）
//!
//! # 终局停机（`terminal_reason`）
//!
//! 致命错误（如飞书 `exceed_conn_limit`：同应用活跃连接数超限）重试无意义。
//! adapter 置 `terminal_reason()` 后 supervisor **停止重连**并把快照切到 `Failed`。
//! 用单一机制表达「停机」而非给 `FrameAction` 加变体——加变体会让所有既有 `match`
//! 变成非穷尽；且它还能覆盖**建连之前**的失败（取端点阶段的致命码，帧动作表达不到）。
//!
//! 典型用法：
//! ```ignore
//! let adapter = Arc::new(MyAdapter::new());
//! let state = ws::start_client(adapter).await?;
//! // ... 监控：state.conn_state_snapshot().await
//! ws::stop_client(state).await;
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

pub mod server;

pub use server::{WsServerHandler, serve as serve_server};

// ==================== 帧发送句柄 ====================

/// 帧发送句柄：pkg 内部连接（client/server）对外的统一出站接口
///
/// 联邦出站 consumer 据此向对端 push 帧；实现方负责串行化写端。
#[async_trait]
pub trait FrameTx: Send + Sync {
    /// 发送一帧文本
    async fn send_text(&self, text: String) -> Result<()>;
    /// 关闭连接
    async fn close(&self) -> Result<()>;
    /// 连接是否仍然存活（尽力判断：写端未关闭即视为存活）
    fn is_alive(&self) -> bool;
}

// ==================== 适配器 trait ====================

/// 单帧处置动作（adapter 对每帧解释后给出的决定）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAction {
    /// 继续接收
    Continue,
    /// 结束本次连接（如服务端应用层 close 通知），由 supervisor 重连
    Reconnect,
}

/// 入站帧（pkg 只区分文本 / 二进制，对内容零假设）
///
/// 与 `Message::Text` / `Message::Binary` 一一对应。WS 控制帧（Ping / Pong / Close）
/// **不**经过此类型——它们的处置是连接层自身的职责。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsFrame {
    /// 文本帧
    Text(String),
    /// 二进制帧
    Binary(Vec<u8>),
}

impl WsFrame {
    /// 帧负载字节数（日志用，避免打印内容）
    pub fn len(&self) -> usize {
        match self {
            WsFrame::Text(t) => t.len(),
            WsFrame::Binary(b) => b.len(),
        }
    }

    /// 是否零长负载
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 出站帧
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsOutFrame {
    /// 文本帧
    Text(String),
    /// 二进制帧
    Binary(Vec<u8>),
    /// 协议级 Ping 控制帧（对端会自动回 Pong）
    Ping(Vec<u8>),
}

/// 单帧处置结果：动作 + 需要**立即回写**的帧
///
/// 「立即回写」是为需要当面应答（ACK）的协议准备的：飞书要求事件在 **3 秒**内回 ACK，
/// 超时服务端重推。回写发生在读循环内、`on_message` 返回之后，因此仍满足
/// 「读循环只入队不做业务」——`replies` 由 adapter 在解析层决定，不涉及业务处理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameOutcome {
    /// 本次连接是否应结束（`Reconnect` → supervisor 重连）
    pub action: FrameAction,
    /// 立即回写的帧（ACK / 自定义 pong 等）
    pub replies: Vec<WsOutFrame>,
}

impl Default for FrameOutcome {
    fn default() -> Self {
        Self::cont()
    }
}

impl FrameOutcome {
    /// 继续接收、不回写
    pub fn cont() -> Self {
        Self {
            action: FrameAction::Continue,
            replies: Vec::new(),
        }
    }

    /// 结束本次连接、不回写
    pub fn reconnect() -> Self {
        Self {
            action: FrameAction::Reconnect,
            replies: Vec::new(),
        }
    }

    /// 附加一个需立即回写的帧
    pub fn with_reply(mut self, reply: WsOutFrame) -> Self {
        self.replies.push(reply);
        self
    }
}

/// 心跳指令（adapter 声明「每隔多久发什么」）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    /// 心跳间隔；`Duration::ZERO` 表示关闭心跳
    pub interval: Duration,
    /// 要发的帧；`None` 表示协议级 Ping 控制帧
    pub frame: Option<WsOutFrame>,
}

/// 重连策略（每轮重连**重新查询**，允许运行期改变间隔）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ReconnectPolicy {
    /// 指数退避（默认，等价既有行为）：1s 起倍增、60s 封顶、±20% 抖动
    #[default]
    Exponential,
    /// 固定间隔（飞书口径：服务端下发 `ReconnectInterval`）
    Fixed {
        /// 重连间隔
        interval: Duration,
        /// 首次重连叠加的随机抖动上限（服务端 `ReconnectNonce`；`None` 不叠加）
        first_jitter: Option<Duration>,
        /// 重连次数上限（`None` = 无限）；达到上限即终局停机
        max_attempts: Option<u64>,
    },
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

    /// 握手请求自定义 header（如联邦 `Authorization: Bearer` 凭证）。
    /// 每次建连/重连都会调用——重连即重新握手鉴权（P0 红线）。
    fn handshake_headers(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    /// 处理一帧入站消息（文本 / 二进制），返回动作与**需立即回写**的帧
    ///
    /// 默认实现：文本转投 [`WsClientAdapter::on_frame`]（既有通路零改动）；
    /// 二进制**留痕不静默**——pkg 组件无二进制语义，需要二进制协议的 adapter
    /// （如飞书 pbbp2）必须覆盖本方法。
    async fn on_message(&self, frame: WsFrame) -> FrameOutcome {
        match frame {
            WsFrame::Text(text) => FrameOutcome {
                action: self.on_frame(text).await,
                replies: Vec::new(),
            },
            WsFrame::Binary(bytes) => {
                log_debug!(
                    "{} ws binary frame ignored by default adapter (len={})",
                    self.name(),
                    bytes.len()
                );
                FrameOutcome::cont()
            }
        }
    }

    /// 心跳指令；返回 `None` 表示关闭心跳
    ///
    /// 默认：30s 间隔 + [`WsClientAdapter::heartbeat_frame`]
    /// （`Some` 发文本帧、`None` 发协议级 Ping）——等价既有行为。
    /// 实现方覆盖本方法即可让**间隔与帧内容在运行期变化**（服务端下发新值时当轮生效）。
    fn heartbeat(&self) -> Option<Heartbeat> {
        Some(Heartbeat {
            interval: HEARTBEAT_INTERVAL,
            frame: self.heartbeat_frame().map(WsOutFrame::Text),
        })
    }

    /// 重连策略；默认指数退避（等价既有行为）
    fn reconnect_policy(&self) -> ReconnectPolicy {
        ReconnectPolicy::Exponential
    }

    /// 终局原因：返回 `Some` 表示**不再重连**（supervisor 停机并把快照切 `Failed`）
    ///
    /// 适用于重试无意义且能定位根因的失败（凭据错、连接数超限）。
    /// supervisor 在**每次连接退出的边界**查询——因此它同时覆盖建连前（取端点）
    /// 与建连后两个阶段，这是帧动作表达不到的。
    fn terminal_reason(&self) -> Option<String> {
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
    /// 终局失败：已停止重连，需人工介入（见 `terminal_reason`）
    Failed,
}

impl WsConnPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            WsConnPhase::Connecting => "connecting",
            WsConnPhase::Connected => "connected",
            WsConnPhase::Reconnecting => "reconnecting",
            WsConnPhase::Failed => "failed",
        }
    }

    /// 是否为终局态（不再自动恢复）
    pub fn is_terminal(&self) -> bool {
        matches!(self, WsConnPhase::Failed)
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
    /// 最近一次收到**任意帧**的时间（Unix 毫秒；`None` = 本连接从未收到）
    ///
    /// 判活字段：「已连接但此值停走」= 对端静默 / 半开连接（TCP 黑洞下心跳写进内核
    /// 缓冲区也会「成功」）。仅用于展示与告警，**不触发断连**（不做无帧判死）。
    pub last_frame_at_ms: Option<i64>,
    /// 累计收到帧数（含控制帧）
    pub frames_received: u64,
    /// 最近一次收到的 close 帧 code（服务端主动关闭的唯一证据）
    pub last_close_code: Option<u16>,
    /// 最近一次收到的 close 帧 reason
    pub last_close_reason: Option<String>,
    /// 终局原因（`Some` ⇒ supervisor 已停止重连）
    pub terminal_reason: Option<String>,
}

impl WsConnState {
    fn new() -> Self {
        Self {
            phase: WsConnPhase::Connecting,
            reconnect_count: 0,
            last_connected_at: None,
            last_frame_at_ms: None,
            frames_received: 0,
            last_close_code: None,
            last_close_reason: None,
            terminal_reason: None,
        }
    }
}

// ==================== WsClientState ====================

/// WebSocket 客户端运行时状态
///
/// 持有 supervisor 任务句柄与关闭信号，用于优雅关闭；
/// 并暴露当前连接的出站句柄（重连后自动切换到新连接）。
pub struct WsClientState {
    /// supervisor 任务句柄（内含退避重连循环）
    supervisor_handle: JoinHandle<()>,
    /// 关闭信号（置 true 即退出）
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// 共享连接状态（监控快照读取入口）
    conn_state: Arc<RwLock<WsConnState>>,
    /// 当前连接的出站句柄（每次建连更新，断开置空）
    tx: Arc<RwLock<Option<Arc<dyn FrameTx>>>>,
}

impl WsClientState {
    /// 读取当前连接状态快照
    pub async fn conn_state_snapshot(&self) -> WsConnState {
        self.conn_state.read().await.clone()
    }

    /// 当前连接是否可用（有活连接且写端存活）
    pub async fn is_connected(&self) -> bool {
        self.tx
            .read()
            .await
            .as_ref()
            .map(|t| t.is_alive())
            .unwrap_or(false)
    }

    /// 同步尽力判断连接是否可用（`try_read` 锁被占用时返回 false）
    ///
    /// 供 `FrameTx::is_alive` 等同步上下文调用。
    pub fn try_is_connected(&self) -> bool {
        self.tx
            .try_read()
            .map(|guard| guard.as_ref().map(|t| t.is_alive()).unwrap_or(false))
            .unwrap_or(false)
    }

    /// 向当前连接发送一帧文本（无活连接时报错，由调用方决定回退策略）
    pub async fn send_text(&self, text: String) -> Result<()> {
        let tx = self.tx.read().await.clone();
        match tx {
            Some(tx) if tx.is_alive() => tx.send_text(text).await,
            _ => Err(err!(ThirdPartyError, "ws client not connected")),
        }
    }
}

// ==================== supervisor ====================

/// 单次连接的退出原因
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnExit {
    /// 是否因 shutdown 信号退出（true 则 supervisor 终止，不重连）
    shutdown: bool,
    /// 本次连接是否曾成功建连（用于重置退避）
    connected: bool,
    /// 终局原因（`Some` ⇒ 不再重连，supervisor 停机）
    terminal: Option<String>,
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
    let tx: Arc<RwLock<Option<Arc<dyn FrameTx>>>> = Arc::new(RwLock::new(None));

    let supervisor_handle = tokio::spawn({
        let conn_state = conn_state.clone();
        let tx_slot = tx.clone();
        async move {
            let name = adapter.name().to_string();
            let mut backoff: Option<Duration> = None;
            let mut has_connected_once = false;
            // 固定间隔策略下的连续重连尝试次数（建连成功即归零）
            let mut attempts: u64 = 0;
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                let exit = run_connection_once(
                    adapter.clone(),
                    conn_state.clone(),
                    tx_slot.clone(),
                    shutdown_rx.clone(),
                )
                .await;
                let exit = match exit {
                    Ok(exit) => exit,
                    Err(e) => {
                        log_warn!("{} ws connection attempt failed: {}", name, e);
                        ConnExit {
                            shutdown: false,
                            connected: false,
                            // 取端点阶段的失败（如连接数超限）也要能触发终局
                            terminal: adapter.terminal_reason(),
                        }
                    }
                };
                if exit.shutdown {
                    break;
                }
                if exit.connected {
                    // 建连成功 → 重置退避与尝试计数；非首次视为一次重连成功
                    backoff = None;
                    attempts = 0;
                    if has_connected_once {
                        conn_state.write().await.reconnect_count += 1;
                    }
                    has_connected_once = true;
                }

                // 终局：adapter 显式置位（凭据错 / 连接数超限等重试无意义的失败）
                if let Some(reason) = exit.terminal {
                    {
                        let mut st = conn_state.write().await;
                        st.phase = WsConnPhase::Failed;
                        st.terminal_reason = Some(reason.clone());
                    }
                    log_error!("{} ws terminal, stop reconnecting: {}", name, reason);
                    break;
                }

                // 重连策略每轮重查（服务端可在运行期改变间隔）
                let delay = match adapter.reconnect_policy() {
                    ReconnectPolicy::Exponential => {
                        let d = next_backoff(backoff);
                        backoff = Some(d);
                        d
                    }
                    ReconnectPolicy::Fixed {
                        interval,
                        first_jitter,
                        max_attempts,
                    } => {
                        attempts += 1;
                        // 重连次数耗尽 → 终局（官方 `reconnectCount` 语义，-1 为无限）
                        if let Some(max) = max_attempts
                            && attempts > max
                        {
                            let reason = format!("reconnect exhausted after {} attempts", max);
                            {
                                let mut st = conn_state.write().await;
                                st.phase = WsConnPhase::Failed;
                                st.terminal_reason = Some(reason.clone());
                            }
                            log_error!("{} ws terminal: {}", name, reason);
                            break;
                        }
                        let mut d = interval;
                        // 仅首次重连叠加抖动，之后固定间隔（官方 `ReconnectNonce` 语义）
                        if attempts == 1
                            && let Some(jitter) = first_jitter
                            && !jitter.is_zero()
                        {
                            use rand::Rng;
                            let ms = jitter.as_millis() as u64;
                            d += Duration::from_millis(rand::thread_rng().gen_range(0..=ms));
                        }
                        d
                    }
                };
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
        tx,
    })
}

/// 停止客户端长连接：发送关闭信号并等待 supervisor 退出
pub async fn stop_client(state: WsClientState) {
    let _ = state.shutdown_tx.send(true);
    // 等待 supervisor 退出（忽略错误：任务可能已退出）
    let _ = state.supervisor_handle.await;
    log_info!("ws client stopped");
}

/// 停止 Arc 持有的客户端（联邦拨号等共享句柄场景）
pub async fn stop_client_shared(state: Arc<WsClientState>) {
    let _ = state.shutdown_tx.send(true);
    // supervisor 退出后 Arc 归零；若他处仍持有则等待退出信号生效即可
    log_info!("ws client stop requested");
}

// ==================== 单次连接生命周期 ====================

/// 单次连接生命周期：取端点 → 建连 → 心跳 + recv
///
/// 返回退出原因；shutdown 信号到达时立即清理退出。
async fn run_connection_once(
    adapter: Arc<dyn WsClientAdapter>,
    conn_state: Arc<RwLock<WsConnState>>,
    tx_slot: Arc<RwLock<Option<Arc<dyn FrameTx>>>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<ConnExit> {
    let name = adapter.name();

    // 1. 获取连接地址（adapter 决定：动态端点 / 静态解析）
    conn_state.write().await.phase = WsConnPhase::Connecting;
    let ws_url = adapter.endpoint().await?;
    log_info!("{} ws connecting to endpoint", name);

    // 2. 建立 WebSocket 连接（注入握手 header——重连即重新握手鉴权）
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request: tokio_tungstenite::tungstenite::handshake::client::Request = ws_url
        .into_client_request()
        .map_err(|e| err!(ThirdPartyError, "{} ws invalid request url: {}", name, e))?;
    for (key, value) in adapter.handshake_headers() {
        let header_value = value
            .parse::<axum::http::HeaderValue>()
            .map_err(|e| err!(ThirdPartyError, "{} ws invalid header {}: {}", name, key, e))?;
        request.headers_mut().insert(key, header_value);
    }
    let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
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

    // 注册出站句柄（出站 push 用；断开时清空）
    let client_tx: Arc<dyn FrameTx> = Arc::new(ClientFrameTx {
        write: write.clone(),
        closed: AtomicBool::new(false),
    });
    *tx_slot.write().await = Some(client_tx.clone());

    // 3. 心跳任务：adapter 每 tick 声明「发什么、间隔多久」（服务端可运行期改变）
    let heartbeat_write = write.clone();
    let heartbeat_shutdown = shutdown_rx.clone();
    let heartbeat_adapter = adapter.clone();
    let heartbeat_name = name.to_string();
    let heartbeat_handle = tokio::spawn(async move {
        let mut shutdown_rx = heartbeat_shutdown;
        loop {
            let hb = heartbeat_adapter.heartbeat();
            // 心跳关闭（None / 零间隔）：仅等 shutdown，不忙等
            let Some(hb) = hb else {
                if wait_shutdown(&mut shutdown_rx).await {
                    break;
                }
                continue;
            };
            if hb.interval.is_zero() {
                if wait_shutdown(&mut shutdown_rx).await {
                    break;
                }
                continue;
            }
            tokio::select! {
                _ = tokio::time::sleep(hb.interval) => {
                    let res = {
                        let mut w = heartbeat_write.lock().await;
                        send_out_frame(&mut w, hb.frame).await
                    };
                    if let Err(e) = res {
                        log_warn!("{} ws heartbeat send failed: {}", heartbeat_name, e);
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
        terminal: None,
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
                        // 任何入站帧（含控制帧）都是「对端还活着」的证据
                        mark_frame_received(&conn_state).await;
                        let outcome = match msg {
                            Message::Text(text) => adapter.on_message(WsFrame::Text(text)).await,
                            Message::Binary(bytes) => {
                                adapter.on_message(WsFrame::Binary(bytes)).await
                            }
                            Message::Close(close) => {
                                // 服务端主动关闭：code/reason 是区分「正常轮换」与
                                // 「连接被顶/冲突」的唯一证据，必须落进快照 + 日志
                                let (code, reason) = record_close(&conn_state, close.as_ref()).await;
                                log_info!(
                                    "{} ws received close frame: code={:?} reason={:?}",
                                    name,
                                    code,
                                    reason
                                );
                                break;
                            }
                            Message::Ping(payload) => {
                                // tungstenite 在读循环内自动回 Pong（无需手工应答），此处仅留痕
                                log_debug!("{} ws recv ping (len={})", name, payload.len());
                                continue;
                            }
                            Message::Pong(payload) => {
                                log_debug!("{} ws recv pong (len={})", name, payload.len());
                                continue;
                            }
                            Message::Frame(_) => continue,
                        };
                        // 立即回写（ACK / 自定义应答）——必须在动作判定前完成，
                        // 否则 Reconnect 会让需要应答的帧（如飞书 3s ACK 窗口）失去应答机会
                        if !outcome.replies.is_empty() {
                            let mut w = write.lock().await;
                            for reply in outcome.replies {
                                if let Err(e) = send_out_frame(&mut w, Some(reply)).await {
                                    log_warn!("{} ws reply send failed: {}", name, e);
                                    break;
                                }
                            }
                        }
                        if outcome.action == FrameAction::Reconnect {
                            log_info!("{} ws frame requested reconnect", name);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        log_error!("{} ws recv error: {}", name, e);
                        break;
                    }
                    None => {
                        log_info!("{} ws stream closed by server without close frame", name);
                        break;
                    }
                }
            }
        }
    }
    // 退出边界判定终局：adapter 已置位终局原因（如收到致命错误码）
    exit.terminal = adapter.terminal_reason();

    // 关闭 write 端并等待心跳任务退出；清空出站句柄
    let _ = client_tx.close().await;
    {
        let mut w = write.lock().await;
        let _ = w.close().await;
    }
    heartbeat_handle.abort();
    let _ = heartbeat_handle.await;
    *tx_slot.write().await = None;
    log_info!("{} ws connection exited", name);
    Ok(exit)
}

// ==================== 连接层辅助 ====================

/// 等待 shutdown 信号；返回 `true` 表示已收到关闭请求（发送端 drop 亦视为关闭）
async fn wait_shutdown(rx: &mut tokio::sync::watch::Receiver<bool>) -> bool {
    loop {
        if *rx.borrow() {
            return true;
        }
        if rx.changed().await.is_err() {
            return true;
        }
    }
}

/// 标记收到一帧（判活证据：时间戳 + 计数）
async fn mark_frame_received(conn_state: &Arc<RwLock<WsConnState>>) {
    let mut st = conn_state.write().await;
    st.last_frame_at_ms = Some(chrono::Utc::now().timestamp_millis());
    st.frames_received = st.frames_received.saturating_add(1);
}

/// 记录 close 帧的 code/reason，返回记录值供日志使用
async fn record_close(
    conn_state: &Arc<RwLock<WsConnState>>,
    close: Option<&tokio_tungstenite::tungstenite::protocol::CloseFrame<'_>>,
) -> (Option<u16>, Option<String>) {
    let mut st = conn_state.write().await;
    match close {
        Some(frame) => {
            st.last_close_code = Some(u16::from(frame.code));
            st.last_close_reason = Some(frame.reason.to_string());
        }
        None => {
            st.last_close_code = None;
            st.last_close_reason = None;
        }
    }
    (st.last_close_code, st.last_close_reason.clone())
}

/// 向写端发送一帧（`None` → 协议级 Ping 控制帧）
async fn send_out_frame(w: &mut TungsteniteSink, frame: Option<WsOutFrame>) -> Result<()> {
    let msg = match frame {
        Some(WsOutFrame::Text(text)) => Message::Text(text),
        Some(WsOutFrame::Binary(bytes)) => Message::Binary(bytes),
        Some(WsOutFrame::Ping(payload)) => Message::Ping(payload),
        None => Message::Ping(Vec::new()),
    };
    w.send(msg)
        .await
        .map_err(|e| err!(ThirdPartyError, "ws send error: {}", e))
}

/// client 端帧发送句柄（内部包 tungstenite 写端）
type TungsteniteSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

struct ClientFrameTx {
    write: Arc<Mutex<TungsteniteSink>>,
    closed: AtomicBool,
}

#[async_trait]
impl FrameTx for ClientFrameTx {
    async fn send_text(&self, text: String) -> Result<()> {
        let mut w = self.write.lock().await;
        w.send(Message::Text(text))
            .await
            .map_err(|e| err!(ThirdPartyError, "ws send error: {}", e))
    }

    async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        let mut w = self.write.lock().await;
        let _ = w.close().await;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
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
        assert_eq!(WsConnPhase::Failed.as_str(), "failed");
        assert!(WsConnPhase::Failed.is_terminal());
        assert!(!WsConnPhase::Connected.is_terminal());
    }

    // ==================== S1 新增能力测试 ====================

    /// 二进制 + 回写 adapter：记录所有入站帧；二进制帧回写 ack
    struct BinaryAckAdapter {
        url: String,
        received: Arc<tokio::sync::Mutex<Vec<WsFrame>>>,
    }

    #[async_trait]
    impl WsClientAdapter for BinaryAckAdapter {
        fn name(&self) -> &str {
            "test-binary"
        }
        async fn endpoint(&self) -> Result<String> {
            Ok(self.url.clone())
        }
        async fn on_frame(&self, text: String) -> FrameAction {
            self.received.lock().await.push(WsFrame::Text(text));
            FrameAction::Continue
        }
        async fn on_message(&self, frame: WsFrame) -> FrameOutcome {
            self.received.lock().await.push(frame.clone());
            match frame {
                WsFrame::Binary(bytes) => FrameOutcome::cont().with_reply(WsOutFrame::Binary(
                    format!("ack:{}", bytes.len()).into_bytes(),
                )),
                WsFrame::Text(_) => FrameOutcome::cont(),
            }
        }
    }

    /// 测试服务端：建连后推一帧二进制，收集对端回帧（首个文本/二进制后停止）
    async fn spawn_binary_server(
        payload: Vec<u8>,
    ) -> (
        String,
        Arc<tokio::sync::Mutex<Vec<Message>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let got: Arc<tokio::sync::Mutex<Vec<Message>>> = Arc::default();
        let got_server = got.clone();
        let handle = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = accept_async(stream).await else {
                return;
            };
            let (mut write, mut read) = ws.split();
            if write.send(Message::Binary(payload)).await.is_err() {
                return;
            }
            while let Some(Ok(msg)) = read.next().await {
                let stop = msg.is_binary() || msg.is_text();
                got_server.lock().await.push(msg);
                if stop {
                    break;
                }
            }
        });
        (format!("ws://{}", addr), got, handle)
    }

    /// 二进制帧投递 adapter；`FrameOutcome.replies` 在读循环内被立即回写
    #[tokio::test(flavor = "multi_thread")]
    async fn binary_frame_delivery_and_reply() {
        let received: Arc<tokio::sync::Mutex<Vec<WsFrame>>> = Arc::default();
        let (url, server_got, server) = spawn_binary_server(vec![1, 2, 3]).await;

        let adapter = Arc::new(BinaryAckAdapter {
            url,
            received: received.clone(),
        });
        let state = start_client(adapter).await.unwrap();

        for _ in 0..50 {
            if !received.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            received.lock().await.clone(),
            vec![WsFrame::Binary(vec![1, 2, 3])]
        );

        // adapter 声明的回写帧应被读循环立即发出
        for _ in 0..50 {
            if !server_got.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let frames = server_got.lock().await.clone();
        assert!(
            frames
                .iter()
                .any(|m| matches!(m, Message::Binary(b) if b == b"ack:3")),
            "expected ack reply frame, got {:?}",
            frames
        );

        // 收帧证据：计数与时间戳（判活字段）
        let snap = state.conn_state_snapshot().await;
        assert!(snap.frames_received >= 1, "snapshot: {:?}", snap);
        assert!(snap.last_frame_at_ms.is_some());

        stop_client(state).await;
        server.abort();
    }

    /// 默认实现下二进制帧不投 `on_frame`，但仍计入判活证据（不静默丢弃）
    #[tokio::test(flavor = "multi_thread")]
    async fn default_adapter_ignores_binary_but_counts_liveness() {
        let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
        let (url, _got, server) = spawn_binary_server(vec![9, 9]).await;
        let adapter = Arc::new(EchoTestAdapter {
            url,
            received: received.clone(),
        });
        let state = start_client(adapter).await.unwrap();

        for _ in 0..50 {
            if state.conn_state_snapshot().await.frames_received > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let snap = state.conn_state_snapshot().await;
        assert_eq!(snap.phase, WsConnPhase::Connected);
        assert!(snap.frames_received >= 1, "snapshot: {:?}", snap);
        // 联邦默认路径：文本通路不被二进制污染（on_frame 未收到任何东西）
        assert!(received.lock().await.is_empty());

        stop_client(state).await;
        server.abort();
    }

    /// 服务端 close 帧的 code/reason 落进快照（区分正常轮换与被顶/冲突的唯一证据）
    #[tokio::test(flavor = "multi_thread")]
    async fn close_frame_recorded_in_snapshot() {
        use tokio_tungstenite::tungstenite::protocol::CloseFrame;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = accept_async(stream).await else {
                return;
            };
            let (mut write, _read) = ws.split();
            let _ = write
                .send(Message::Close(Some(CloseFrame {
                    // 4001 = 应用自定义「被顶/踢出」：`CloseCode` 是私有 re-export，
                    // 用 `.into()` 交给类型推断（目标类型由 CloseFrame.code 决定）
                    code: 4001u16.into(),
                    reason: std::borrow::Cow::Borrowed("kicked"),
                })))
                .await;
        });

        let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
        let adapter = Arc::new(EchoTestAdapter {
            url: format!("ws://{}", addr),
            received,
        });
        let state = start_client(adapter).await.unwrap();

        for _ in 0..50 {
            if state.conn_state_snapshot().await.last_close_code.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let snap = state.conn_state_snapshot().await;
        assert_eq!(snap.last_close_code, Some(4001));
        assert_eq!(snap.last_close_reason.as_deref(), Some("kicked"));

        stop_client(state).await;
        server.abort();
    }

    /// 终局停机 adapter：端点永远失败，且自报终局原因
    struct TerminalAdapter {
        terminal: Arc<std::sync::Mutex<Option<String>>>,
    }

    #[async_trait]
    impl WsClientAdapter for TerminalAdapter {
        fn name(&self) -> &str {
            "test-terminal"
        }
        async fn endpoint(&self) -> Result<String> {
            Err(err!(ThirdPartyError, "endpoint unavailable"))
        }
        async fn on_frame(&self, _text: String) -> FrameAction {
            FrameAction::Continue
        }
        fn terminal_reason(&self) -> Option<String> {
            self.terminal.lock().ok().and_then(|g| g.clone())
        }
    }

    /// `terminal_reason` 置位 → supervisor 停机（`Failed`）且不再重连
    #[tokio::test(flavor = "multi_thread")]
    async fn terminal_reason_stops_reconnect() {
        let adapter = Arc::new(TerminalAdapter {
            terminal: Arc::new(std::sync::Mutex::new(Some("exceed_conn_limit".to_string()))),
        });
        let state = start_client(adapter).await.unwrap();

        for _ in 0..50 {
            if state.conn_state_snapshot().await.phase == WsConnPhase::Failed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let snap = state.conn_state_snapshot().await;
        assert_eq!(snap.phase, WsConnPhase::Failed, "snapshot: {:?}", snap);
        assert_eq!(snap.terminal_reason.as_deref(), Some("exceed_conn_limit"));

        // 停机不再重连：等待远超指数退避首轮（约 2s），状态保持 Failed
        tokio::time::sleep(Duration::from_millis(2500)).await;
        let snap = state.conn_state_snapshot().await;
        assert_eq!(snap.phase, WsConnPhase::Failed);
        assert_eq!(snap.reconnect_count, 0);

        stop_client(state).await;
    }

    /// 心跳间隔热变更 adapter（运行期可改，模拟服务端下发新参数）
    struct TuningHeartbeatAdapter {
        url: String,
        interval_ms: Arc<std::sync::atomic::AtomicU64>,
    }

    #[async_trait]
    impl WsClientAdapter for TuningHeartbeatAdapter {
        fn name(&self) -> &str {
            "test-heartbeat"
        }
        async fn endpoint(&self) -> Result<String> {
            Ok(self.url.clone())
        }
        async fn on_frame(&self, _text: String) -> FrameAction {
            FrameAction::Continue
        }
        fn heartbeat(&self) -> Option<Heartbeat> {
            Some(Heartbeat {
                interval: Duration::from_millis(self.interval_ms.load(Ordering::SeqCst)),
                frame: Some(WsOutFrame::Text("hb".to_string())),
            })
        }
    }

    /// 心跳间隔由 adapter 每 tick 提供 ⇒ 运行期改变当轮生效
    #[tokio::test(flavor = "multi_thread")]
    async fn heartbeat_interval_takes_effect_live() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let got: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
        let got_server = got.clone();
        let server = tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(ws) = accept_async(stream).await else {
                return;
            };
            let (_write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    got_server.lock().await.push(text);
                }
            }
        });

        let interval_ms = Arc::new(std::sync::atomic::AtomicU64::new(30));
        let adapter = Arc::new(TuningHeartbeatAdapter {
            url: format!("ws://{}", addr),
            interval_ms: interval_ms.clone(),
        });
        let state = start_client(adapter).await.unwrap();

        // 30ms 间隔：300ms 内应收到多帧
        tokio::time::sleep(Duration::from_millis(300)).await;
        let fast = got.lock().await.len();
        assert!(
            fast >= 4,
            "expected several heartbeats at 30ms, got {}",
            fast
        );

        // 改为 1000ms：当轮生效（旧间隔最多再补一帧）
        interval_ms.store(1000, Ordering::SeqCst);
        let mark = got.lock().await.len();
        tokio::time::sleep(Duration::from_millis(250)).await;
        let after = got.lock().await.len();
        assert!(
            after - mark <= 1,
            "heartbeat interval change not effective: {} -> {}",
            mark,
            after
        );

        stop_client(state).await;
        server.abort();
    }
}
