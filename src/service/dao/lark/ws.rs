//! 飞书 WebSocket 长连接适配器（`pbbp2` / protobuf）
//!
//! 连接生命周期（建连 / 重连 / 心跳 / 读循环 / 状态快照 / 优雅关闭）由通用组件
//! `pkg::ws` 承载；本文件只剩飞书协议语义：
//!
//! - **取连接地址**：`POST {API_BASE}/callback/ws/endpoint`（**单数** `/endpoint`、
//!   **不带** `/open-apis`），body `{AppID, AppSecret}` —— 长连接只认**应用凭证**，
//!   **不接受** `tenant_access_token`（Bearer 是 REST 那条线的产物）
//! - **帧**：`pbbp2` protobuf 二进制（`method=0` 控制帧 / `method=1` 数据帧），
//!   字段与 tag 表见 [`super::pbbp2`]
//! - **心跳**：pbbp2 控制帧 `type=ping`，间隔由服务端 `ClientConfig.PingInterval` 下发，
//!   控制帧 `type=pong` 会**再覆盖一次**（运行期热生效，见 `heartbeat()`）
//! - **数据帧**：`type=event` → 分片重组 → payload JSON → `LarkInboundEvent` → publish，
//!   随即**在读循环内回 ACK**（官方 3 秒未 ACK 即重推）
//! - **重连**：固定间隔（服务端 `ReconnectInterval`），首次叠加 `ReconnectNonce` 抖动
//! - **致命码**：`exceed_conn_limit` / `auth_failed` 等重试无意义 → 置 `terminal_reason`
//!   交给 supervisor 停机（健康页显示 `failed`）
//!
//! 协议依据：官方 Node SDK `@larksuiteoapi/node-sdk@1.74.0`（`lib/index.js`
//! `pullConnectConfig` L102306 / `communicate` L102595 / `handleControlData` L102633 /
//! `handleEventData` L102665 / `DataCache.mergeData` L102010），与官方 Go SDK `v3_main` 互证。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use prost::Message;
use serde::Deserialize;

use super::LarkAppCredentials;
use super::pbbp2::{self, ConnConfigError, Frame, error_code, header_key, msg_type};
use crate::models::events::{LarkInboundEvent, LarkMessageEvent};
use crate::pkg::RequestContext;
use crate::pkg::ws::{
    FrameAction, FrameOutcome, Heartbeat, ReconnectPolicy, WsClientAdapter, WsFrame, WsOutFrame,
};
use common::error::{Result, err};

// ==================== 常量 ====================

/// 飞书开放平台接入域（D4 未拍板 ⇒ 本轮只支持国内版，抽常量便于后续配置化）
const API_BASE: &str = "https://open.feishu.cn";
/// 取长连接地址路径（官方 `WS_ENDPOINT_URI`：单数 `/endpoint`，**不带** `/open-apis`）
const PATH_WS_ENDPOINT: &str = "/callback/ws/endpoint";
/// 取连接地址请求超时（官方 15s）
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(15);
/// 单帧解码上限（防御性：正常事件帧远小于此；超限丢弃并留痕）
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// 分片数上限（防御：`sum` 来自对端，直接按它分配数组可被撑爆内存）
const MAX_FRAGMENT_COUNT: usize = 256;
/// 分片槽数量上限（防御：多 `message_id` 轰炸）
const MAX_FRAGMENT_SLOTS: usize = 1024;
/// 分片缓存 TTL（对齐官方 10s：超时未齐的分片丢弃，避免内存滞留）
const FRAGMENT_TTL: Duration = Duration::from_secs(10);
/// 服务端未下发参数时的兜底（对齐官方 `WSConfig` 默认值）
const DEFAULT_PING_INTERVAL_SECS: u64 = 120;
const DEFAULT_RECONNECT_INTERVAL_SECS: u64 = 120;
const DEFAULT_RECONNECT_NONCE_SECS: u64 = 30;
/// 日志中事件正文的最大字节数（避免超大 body 灌爆日志）
const LOG_BODY_LIMIT: usize = 512;
/// 订阅的事件类型（本轮只处理私信文本）
const EVENT_TYPE_MESSAGE: &str = "im.message.receive_v1";

// ==================== 对外类型（生命周期部分由 pkg::ws 提供） ====================

/// 飞书 WS 客户端运行时状态（= pkg 通用客户端状态）
pub type WsState = crate::pkg::ws::WsClientState;

pub use crate::pkg::ws::{WsConnPhase, WsConnState};

/// 停止事件循环（pkg 通用关闭）
pub use crate::pkg::ws::stop_client as stop_event_loop;

// ==================== 连接地址响应 ====================

/// 服务端下发的连接参数（取端点响应 `data.ClientConfig`；pong payload 会再覆盖）
///
/// 官方：`PingInterval * 1000` ⇒ 单位是**秒**。缺字段表示「不下发」，保留原值。
#[derive(Debug, Clone, Default, Deserialize)]
struct ClientConfig {
    #[serde(rename = "PingInterval")]
    ping_interval: Option<u64>,
    #[serde(rename = "ReconnectInterval")]
    reconnect_interval: Option<u64>,
    #[serde(rename = "ReconnectCount")]
    reconnect_count: Option<i64>,
    #[serde(rename = "ReconnectNonce")]
    reconnect_nonce: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WsEndpointData {
    #[serde(rename = "URL")]
    url: String,
    #[serde(rename = "ClientConfig")]
    client_config: Option<ClientConfig>,
}

#[derive(Debug, Deserialize)]
struct WsEndpointResp {
    code: i32,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<WsEndpointData>,
}

// ==================== app_id 形态校验 ====================

/// app_id 前缀（飞书自建应用 ID 形态 `cli_` + 16 位十六进制）
const APP_ID_PREFIX: &str = "cli_";

/// 校验 app_id 形态，返回**告警文案**（`None` = 形态合规）
///
/// 取舍说明：形态不合规**不拒绝建连**，只打 `warn` 后继续尝试。原因是"16 位十六进制"
/// 这一形态来自官方 SDK 示例与文档，但未在真机上穷举验证过；若假设过严而本地直接拒绝，
/// 会把**合法配置判死**（后果是功能不可用），而继续发一次请求的最坏后果只是从响应码
/// 得到真实原因（后果轻微）。两者不对称，故选择"告警但继续"。
pub fn check_app_id(app_id: &str) -> Option<String> {
    let Some(hex) = app_id.strip_prefix(APP_ID_PREFIX) else {
        return Some(format!(
            "app_id 未以 `{}` 开头（疑似填成了别的字段）",
            APP_ID_PREFIX
        ));
    };
    if hex.len() != 16 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!(
            "app_id 长度/字符集不符预期（期望 `{}` + 16 位十六进制，实得 {} 位）",
            APP_ID_PREFIX,
            hex.len()
        ));
    }
    None
}

// ==================== 分片重组 ====================

/// 分片重组结果
#[derive(Debug, PartialEq, Eq)]
enum MergeOutcome {
    /// 分片已齐 → 按 `seq` 顺序拼接后的原始字节
    Complete(Vec<u8>),
    /// 尚未齐全（等待后续分片；**不回 ACK**，让服务端重推缺失分片）
    Pending,
    /// 元数据非法（调用方留痕后丢弃）
    Invalid,
}

#[derive(Debug)]
struct FragmentSlot {
    /// 各分片负载（`None` = 未到）
    parts: Vec<Option<Vec<u8>>>,
    /// 首个分片到达时间（TTL 清理依据）
    first_seen: Instant,
}

/// 分片缓存（官方 `DataCache`：按 `message_id` 建槽，10s 过期）
#[derive(Debug, Default)]
struct FragmentCache {
    slots: HashMap<String, FragmentSlot>,
}

/// 合并一个分片（官方 `mergeData` 语义）
///
/// 元数据规则：
/// - `sum` / `seq` **都缺失** → 视为**单片**（`sum=1, seq=0`）
///   ——比官方更宽容：官方此处 `Number(undefined)=NaN` 会抛错丢帧，
///   若线上存在「未带分片元数据的整帧事件」，照抄官方会导致**完全收不到事件**；
/// - 只带其中一个、非数字、`sum == 0`、`seq >= sum`、`sum` 超上限 → [`MergeOutcome::Invalid`]
/// - 后续分片的 `sum` 与首片不一致 → `Invalid`（官方同）
fn merge_fragment(
    cache: &mut FragmentCache,
    message_id: &str,
    sum: Option<&str>,
    seq: Option<&str>,
    payload: &[u8],
) -> MergeOutcome {
    // TTL 清理：超时未齐的分片直接丢弃，避免内存滞留
    cache
        .slots
        .retain(|_, slot| slot.first_seen.elapsed() < FRAGMENT_TTL);

    let (sum, seq) = match (sum, seq) {
        (None, None) => (1usize, 0usize),
        (Some(raw_sum), Some(raw_seq)) => {
            let (Ok(sum), Ok(seq)) = (raw_sum.parse::<usize>(), raw_seq.parse::<usize>()) else {
                return MergeOutcome::Invalid;
            };
            (sum, seq)
        }
        _ => return MergeOutcome::Invalid,
    };
    if sum == 0 || sum > MAX_FRAGMENT_COUNT || seq >= sum {
        return MergeOutcome::Invalid;
    }
    // 多片必须有分片键，否则不同事件会串味
    if sum > 1 && message_id.is_empty() {
        return MergeOutcome::Invalid;
    }

    match cache.slots.get_mut(message_id) {
        None => {
            if cache.slots.len() >= MAX_FRAGMENT_SLOTS {
                return MergeOutcome::Invalid;
            }
            let mut parts = vec![None; sum];
            parts[seq] = Some(payload.to_vec());
            cache.slots.insert(
                message_id.to_string(),
                FragmentSlot {
                    parts,
                    first_seen: Instant::now(),
                },
            );
        }
        Some(slot) => {
            if slot.parts.len() != sum {
                return MergeOutcome::Invalid;
            }
            slot.parts[seq] = Some(payload.to_vec());
        }
    }

    let Some(slot) = cache.slots.get(message_id) else {
        return MergeOutcome::Pending;
    };
    if slot.parts.iter().any(|p| p.is_none()) {
        return MergeOutcome::Pending;
    }
    let mut merged = Vec::new();
    for bytes in slot.parts.iter().flatten() {
        merged.extend_from_slice(bytes);
    }
    cache.slots.remove(message_id);
    MergeOutcome::Complete(merged)
}

// ==================== URL query 解析 ====================

/// 从连接地址的 query 解析 `service_id` / `device_id`（官方 `qs.parse(URL)`）
///
/// - `service_id`：ping 帧的 `service` 字段必需
/// - `device_id`：仅诊断展示
fn parse_conn_query(url: &str) -> (i32, String) {
    let Some((_, query)) = url.split_once('?') else {
        return (0, String::new());
    };
    let mut service_id = 0i32;
    let mut device_id = String::new();
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "service_id" => service_id = value.parse().unwrap_or(0),
            "device_id" => device_id = value.to_string(),
            _ => {}
        }
    }
    (service_id, device_id)
}

// ==================== 适配器 ====================

/// 适配器运行参数（取端点时刷新；pong 覆盖四字段）
#[derive(Debug, Clone)]
struct WsParams {
    /// 连接 URL 里的 `service_id`（ping 帧需要）
    service_id: i32,
    /// 连接 URL 里的 `device_id`（诊断用）
    device_id: String,
    ping_interval_secs: u64,
    reconnect_interval_secs: u64,
    reconnect_nonce_secs: u64,
    /// `None` = 无限重连（官方 `ReconnectCount = -1`）
    reconnect_count: Option<u64>,
}

impl Default for WsParams {
    fn default() -> Self {
        Self {
            service_id: 0,
            device_id: String::new(),
            ping_interval_secs: DEFAULT_PING_INTERVAL_SECS,
            reconnect_interval_secs: DEFAULT_RECONNECT_INTERVAL_SECS,
            reconnect_nonce_secs: DEFAULT_RECONNECT_NONCE_SECS,
            reconnect_count: None,
        }
    }
}

impl WsParams {
    /// 应用下发的连接参数（缺失字段保留原值）
    fn apply_config(&mut self, cfg: &ClientConfig) {
        if let Some(v) = cfg.ping_interval {
            self.ping_interval_secs = v;
        }
        if let Some(v) = cfg.reconnect_interval {
            self.reconnect_interval_secs = v;
        }
        if let Some(v) = cfg.reconnect_nonce {
            self.reconnect_nonce_secs = v;
        }
        if let Some(v) = cfg.reconnect_count {
            // 负数（官方 -1）表示无限重连
            self.reconnect_count = u64::try_from(v).ok();
        }
    }
}

/// payload 截断（日志安全）
fn truncate_bytes(bytes: &[u8]) -> String {
    let limit = bytes.len().min(LOG_BODY_LIMIT);
    let mut text = String::from_utf8_lossy(&bytes[..limit]).to_string();
    if bytes.len() > limit {
        text.push_str(&format!("...(+{} bytes)", bytes.len() - limit));
    }
    text
}

/// 取连接地址 + 服务端下发的连接参数
///
/// 官方 `pullConnectConfig`：`POST`，头 `locale: zh` + UA，body `{AppID, AppSecret}`，
/// 超时 15s；`code != 0` 按 [`pbbp2::classify_config_error`] 分类（只有 `internal_error`
/// 可重试，其余为终局）。
async fn fetch_ws_endpoint(
    http: &reqwest::Client,
    app_id: &str,
    app_secret: &str,
) -> std::result::Result<(String, ClientConfig), ConnConfigError> {
    let url = format!("{}{}", API_BASE, PATH_WS_ENDPOINT);
    let resp = http
        .post(&url)
        .header("locale", "zh")
        .json(&serde_json::json!({ "AppID": app_id, "AppSecret": app_secret }))
        .timeout(ENDPOINT_TIMEOUT)
        .send()
        .await
        .map_err(|e| ConnConfigError::Retryable(format!("http error: {}", e)))?
        .json::<WsEndpointResp>()
        .await
        .map_err(|e| ConnConfigError::Retryable(format!("response parse error: {}", e)))?;

    if resp.code != error_code::OK {
        return Err(pbbp2::classify_config_error(resp.code, &resp.msg));
    }
    let data = resp
        .data
        .ok_or_else(|| ConnConfigError::Retryable("empty data".to_string()))?;
    if data.url.is_empty() {
        return Err(ConnConfigError::Retryable("empty URL".to_string()));
    }
    Ok((data.url, data.client_config.unwrap_or_default()))
}

/// 飞书 WS 适配器：实现 pkg 通用客户端 trait，只含飞书协议语义
struct LarkWsAdapter {
    http: reqwest::Client,
    app_id: String,
    app_secret: String,
    /// 服务端下发参数（取端点刷新 / pong 覆盖）
    params: Arc<RwLock<WsParams>>,
    /// 终局原因（置位后 supervisor 停机；取端点成功后清除以便人工修好后自愈）
    terminal: Arc<Mutex<Option<String>>>,
    /// 分片缓存
    fragments: Arc<Mutex<FragmentCache>>,
}

impl LarkWsAdapter {
    fn new(http: reqwest::Client, app_id: String, app_secret: String) -> Self {
        Self {
            http,
            app_id,
            app_secret,
            params: Arc::new(RwLock::new(WsParams::default())),
            terminal: Arc::new(Mutex::new(None)),
            fragments: Arc::new(Mutex::new(FragmentCache::default())),
        }
    }

    /// 参数快照（锁中毒时退化为兜底默认值，不让监控路径 panic）
    fn params_snapshot(&self) -> WsParams {
        self.params.read().map(|p| p.clone()).unwrap_or_default()
    }

    /// 置位终局原因（只置一次，保留首个根因）
    fn set_terminal(&self, reason: impl Into<String>) {
        if let Ok(mut t) = self.terminal.lock()
            && t.is_none()
        {
            *t = Some(reason.into());
        }
    }

    /// 控制帧处置：握手三字段留痕 + pong 覆盖运行参数（官方 `handleControlData`）
    fn handle_control(&self, frame: &Frame) {
        // 握手诊断字段：服务端在控制帧下发，是「建连握手为什么失败」的唯一线索
        if let Some(status) = frame.header(header_key::HANDSHAKE_STATUS) {
            log_info!(
                "lark ws handshake: status={} msg={:?} autherrcode={:?}",
                status,
                frame.header(header_key::HANDSHAKE_MSG),
                frame.header(header_key::HANDSHAKE_AUTHERRCODE)
            );
        }

        match frame.msg_type() {
            Some(msg_type::PING) => {
                log_debug!("lark ws recv app-layer ping");
            }
            Some(msg_type::PONG) => {
                let Some(text) = frame.payload_text() else {
                    log_warn!("lark ws pong payload is not utf-8 (ignored)");
                    return;
                };
                match serde_json::from_str::<ClientConfig>(text) {
                    Ok(cfg) => {
                        if let Ok(mut params) = self.params.write() {
                            params.apply_config(&cfg);
                            log_info!(
                                "lark ws params updated by pong: ping={}s reconnect={}s nonce={}s max_attempts={:?}",
                                params.ping_interval_secs,
                                params.reconnect_interval_secs,
                                params.reconnect_nonce_secs,
                                params.reconnect_count
                            );
                        }
                    }
                    Err(e) => {
                        log_warn!(
                            "lark ws invalid pong payload (ignored): {} body={}",
                            e,
                            text
                        );
                    }
                }
            }
            other => log_debug!("lark ws recv control frame type={:?}", other),
        }
    }

    /// 构造 ACK 回写帧（官方形状：复用入帧 + `biz_rt` + `{"code":...}`）
    fn ack_reply(&self, inbound: &Frame, code: u16, started: Instant) -> WsOutFrame {
        // 官方写的是**负**耗时（`String(startTime - endTime)`），此处照抄其字面形态
        let biz_rt = -(started.elapsed().as_millis() as i64);
        WsOutFrame::Binary(inbound.ack(code, biz_rt).encode_to_vec())
    }

    /// 数据帧处置：分片合并 → 事件解析 → publish → 立即回 ACK
    async fn handle_data(&self, frame: &Frame) -> FrameOutcome {
        if frame.msg_type() != Some(msg_type::EVENT) {
            // card 等其它数据帧：留痕（不静默）且不回 ACK（非我方订阅的数据类型）
            log_info!(
                "lark ws data frame with unsupported type={:?} (ignored, no ack)",
                frame.msg_type()
            );
            return FrameOutcome::cont();
        }

        let started = Instant::now();
        let message_id = frame.header(header_key::MESSAGE_ID).unwrap_or_default();
        let trace_id = frame.header(header_key::TRACE_ID).unwrap_or_default();

        // 分片合并（临界区不含 await：std 锁不跨 await 持有）
        let merged = {
            let mut cache = match self.fragments.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    log_error!("lark ws fragment cache poisoned (recovered)");
                    poisoned.into_inner()
                }
            };
            merge_fragment(
                &mut cache,
                message_id,
                frame.header(header_key::SUM),
                frame.header(header_key::SEQ),
                &frame.payload,
            )
        };

        let bytes = match merged {
            MergeOutcome::Complete(bytes) => bytes,
            MergeOutcome::Pending => {
                log_debug!(
                    "lark ws event fragment pending: message_id={} seq={:?}/sum={:?}",
                    message_id,
                    frame.header(header_key::SEQ),
                    frame.header(header_key::SUM)
                );
                // 未齐不回 ACK：让服务端重推缺失分片（对齐官方）
                return FrameOutcome::cont();
            }
            MergeOutcome::Invalid => {
                log_error!(
                    "lark ws invalid fragment metadata (dropped): message_id={} sum={:?} seq={:?}",
                    message_id,
                    frame.header(header_key::SUM),
                    frame.header(header_key::SEQ)
                );
                return FrameOutcome::cont();
            }
        };

        let event: LarkMessageEvent = match serde_json::from_slice(&bytes) {
            Ok(event) => event,
            Err(e) => {
                // 解析失败必须留痕：否则症状与「上游没数据」完全同形
                log_error!(
                    "lark ws event payload parse failed: message_id={} trace_id={} err={} body={}",
                    message_id,
                    trace_id,
                    e,
                    truncate_bytes(&bytes)
                );
                // 不回 ACK（对齐官方 mergeData 抛错路径）
                return FrameOutcome::cont();
            }
        };

        log_info!(
            "lark ws received event: id={} type={} message_id={}",
            event.header.event_id,
            event.header.event_type,
            message_id
        );

        // 非订阅事件：留痕后 ACK（避免服务端对无关事件持续重推）
        if event.header.event_type != EVENT_TYPE_MESSAGE {
            log_info!(
                "lark ws skip non-message event: id={} type={}",
                event.header.event_id,
                event.header.event_type
            );
            return FrameOutcome::cont().with_reply(self.ack_reply(frame, 200, started));
        }

        // publish 入队即返回（读循环不做业务）
        let aop_event = LarkInboundEvent {
            app_id: self.app_id.clone(),
            event,
        };
        let ctx = RequestContext::new_system();
        crate::pkg::aop::registry().publish(&ctx, aop_event).await;

        FrameOutcome::cont().with_reply(self.ack_reply(frame, 200, started))
    }
}

#[async_trait::async_trait]
impl WsClientAdapter for LarkWsAdapter {
    fn name(&self) -> &str {
        "lark"
    }

    /// 每次建连/重连取地址：同时刷新服务端下发的连接参数
    async fn endpoint(&self) -> Result<String> {
        if let Some(warning) = check_app_id(&self.app_id) {
            log_warn!("lark ws app_id 形态可疑（继续尝试）: {}", warning);
        }

        match fetch_ws_endpoint(&self.http, &self.app_id, &self.app_secret).await {
            Ok((url, cfg)) => {
                // 取到配置即视为「配置侧已恢复」：清掉上次的终局标记（人工修好后自愈）
                if let Ok(mut t) = self.terminal.lock()
                    && t.is_some()
                {
                    log_info!("lark ws terminal cleared after endpoint fetch succeeded");
                    *t = None;
                }
                let (service_id, device_id) = parse_conn_query(&url);
                if let Ok(mut params) = self.params.write() {
                    params.apply_config(&cfg);
                    params.service_id = service_id;
                    params.device_id = device_id.clone();
                    log_info!(
                        "lark ws endpoint acquired: service_id={} device_id={} ping={}s reconnect={}s nonce={}s max_attempts={:?}",
                        params.service_id,
                        params.device_id,
                        params.ping_interval_secs,
                        params.reconnect_interval_secs,
                        params.reconnect_nonce_secs,
                        params.reconnect_count
                    );
                }
                Ok(url)
            }
            Err(ConnConfigError::Retryable(msg)) => {
                log_warn!("lark ws endpoint failed (retryable): {}", msg);
                Err(err!(ThirdPartyError, "lark ws endpoint failed: {}", msg))
            }
            Err(ConnConfigError::Terminal(msg)) => {
                log_error!("lark ws endpoint failed (terminal): {}", msg);
                self.set_terminal(format!("endpoint config rejected: {}", msg));
                Err(err!(ThirdPartyError, "lark ws endpoint failed: {}", msg))
            }
        }
    }

    /// 飞书协议走二进制帧通路，本方法仅为满足 trait 契约而存在
    ///
    /// 文本帧由 [`WsClientAdapter::on_message`] 拦截并留痕；若真走到这里说明调用方
    /// 绕过了 `on_message`，同样留痕不静默。
    async fn on_frame(&self, text: String) -> FrameAction {
        log_warn!(
            "lark ws text frame reached on_frame (len={}), ignored",
            text.len()
        );
        FrameAction::Continue
    }

    /// 飞书协议是二进制帧；文本帧只可能来自对端异常，留痕后忽略（不静默）
    async fn on_message(&self, frame: WsFrame) -> FrameOutcome {
        let bytes = match frame {
            WsFrame::Binary(bytes) => bytes,
            WsFrame::Text(text) => {
                log_warn!(
                    "lark ws unexpected text frame (len={}) ignored: {}",
                    text.len(),
                    truncate_bytes(text.as_bytes())
                );
                return FrameOutcome::cont();
            }
        };

        if bytes.len() > MAX_FRAME_BYTES {
            log_error!(
                "lark ws frame exceeds size limit (len={} > {}), dropped",
                bytes.len(),
                MAX_FRAME_BYTES
            );
            return FrameOutcome::cont();
        }

        let frame = match Frame::decode(bytes.as_slice()) {
            Ok(frame) => frame,
            Err(e) => {
                log_error!(
                    "lark ws frame decode failed (len={}): {} body={}",
                    bytes.len(),
                    e,
                    truncate_bytes(&bytes)
                );
                return FrameOutcome::cont();
            }
        };

        if frame.is_control() {
            self.handle_control(&frame);
            return FrameOutcome::cont();
        }
        if !frame.is_data() {
            log_warn!("lark ws unknown frame method={} (ignored)", frame.method);
            return FrameOutcome::cont();
        }
        self.handle_data(&frame).await
    }

    /// 心跳：pbbp2 控制帧 `type=ping`，间隔取服务端下发值（每 tick 重查 ⇒ pong 改了当轮生效）
    fn heartbeat(&self) -> Option<Heartbeat> {
        let params = self.params_snapshot();
        if params.ping_interval_secs == 0 {
            // 服务端显式下发 0 = 关闭心跳
            return None;
        }
        Some(Heartbeat {
            interval: Duration::from_secs(params.ping_interval_secs),
            frame: Some(WsOutFrame::Binary(
                Frame::ping(params.service_id).encode_to_vec(),
            )),
        })
    }

    /// 重连：固定间隔 + 首次抖动 + 次数上限（全部来自服务端下发）
    fn reconnect_policy(&self) -> ReconnectPolicy {
        let params = self.params_snapshot();
        ReconnectPolicy::Fixed {
            // 间隔下限 1s：服务端若下发 0，固定零间隔会变成忙重连
            interval: Duration::from_secs(params.reconnect_interval_secs.max(1)),
            first_jitter: Some(Duration::from_secs(params.reconnect_nonce_secs)),
            max_attempts: params.reconnect_count,
        }
    }

    fn terminal_reason(&self) -> Option<String> {
        self.terminal.lock().ok().and_then(|t| t.clone())
    }
}

// ==================== 启动入口 ====================

/// 启动 WebSocket 事件循环（supervisor 模式，由 pkg::ws 驱动）
///
/// 重连间隔、心跳间隔与次数上限均由服务端 `ClientConfig` 下发（`pkg::ws` 每轮重查），
/// 致命码由 [`WsClientAdapter::terminal_reason`] 交给 supervisor 停机。
pub async fn start_event_loop(
    http: reqwest::Client,
    credentials: LarkAppCredentials,
) -> Result<WsState> {
    let adapter = Arc::new(LarkWsAdapter::new(
        http,
        credentials.app_id,
        credentials.app_secret,
    ));
    crate::pkg::ws::start_client(adapter).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_http() -> reqwest::Client {
        crate::pkg::http::presets::outbound()
            .build()
            .expect("构建测试 HTTP 客户端失败")
    }

    fn test_adapter() -> LarkWsAdapter {
        LarkWsAdapter::new(test_http(), "cli_test".to_string(), "secret".to_string())
    }

    /// 单片（`sum=1, seq=0`，官方未分片事件形态）
    #[test]
    fn merge_single_fragment() {
        let mut cache = FragmentCache::default();
        let out = merge_fragment(&mut cache, "m1", Some("1"), Some("0"), b"hello");
        assert_eq!(out, MergeOutcome::Complete(b"hello".to_vec()));
        assert!(cache.slots.is_empty(), "合并完成后应释放槽位");
    }

    /// 未带分片元数据 → 视为单片（比官方更宽容，避免整帧事件被丢）
    #[test]
    fn merge_without_metadata_treated_as_single() {
        let mut cache = FragmentCache::default();
        let out = merge_fragment(&mut cache, "", None, None, b"whole");
        assert_eq!(out, MergeOutcome::Complete(b"whole".to_vec()));
    }

    /// 多片乱序到达 → 按 seq 顺序拼接
    #[test]
    fn merge_multi_fragment_out_of_order() {
        let mut cache = FragmentCache::default();
        assert_eq!(
            merge_fragment(&mut cache, "m2", Some("3"), Some("2"), b"C"),
            MergeOutcome::Pending
        );
        assert_eq!(
            merge_fragment(&mut cache, "m2", Some("3"), Some("0"), b"A"),
            MergeOutcome::Pending
        );
        assert_eq!(
            merge_fragment(&mut cache, "m2", Some("3"), Some("1"), b"B"),
            MergeOutcome::Complete(b"ABC".to_vec())
        );
        assert!(cache.slots.is_empty());
    }

    /// 非法元数据：只带一个 / 非数字 / sum=0 / seq 越界 / sum 不一致 / 超上限
    #[test]
    fn merge_invalid_metadata() {
        let mut cache = FragmentCache::default();
        assert_eq!(
            merge_fragment(&mut cache, "m", Some("2"), None, b"x"),
            MergeOutcome::Invalid
        );
        assert_eq!(
            merge_fragment(&mut cache, "m", Some("abc"), Some("0"), b"x"),
            MergeOutcome::Invalid
        );
        assert_eq!(
            merge_fragment(&mut cache, "m", Some("0"), Some("0"), b"x"),
            MergeOutcome::Invalid
        );
        assert_eq!(
            merge_fragment(&mut cache, "m", Some("2"), Some("2"), b"x"),
            MergeOutcome::Invalid
        );
        assert_eq!(
            merge_fragment(
                &mut cache,
                "m",
                Some(&(MAX_FRAGMENT_COUNT + 1).to_string()),
                Some("0"),
                b"x"
            ),
            MergeOutcome::Invalid
        );
        // 首片建槽后，后续 sum 不一致 → Invalid
        assert_eq!(
            merge_fragment(&mut cache, "m3", Some("2"), Some("0"), b"x"),
            MergeOutcome::Pending
        );
        assert_eq!(
            merge_fragment(&mut cache, "m3", Some("3"), Some("1"), b"x"),
            MergeOutcome::Invalid
        );
        // 多片但缺分片键 → Invalid
        assert_eq!(
            merge_fragment(&mut cache, "", Some("2"), Some("0"), b"x"),
            MergeOutcome::Invalid
        );
    }

    /// 连接 URL query 解析出 service_id / device_id
    #[test]
    fn parse_query_extracts_ids() {
        let (sid, did) =
            parse_conn_query("wss://gw.example.com/ws?device_id=dev-1&service_id=42&x=1");
        assert_eq!(sid, 42);
        assert_eq!(did, "dev-1");
        // 无 query / 无相关字段
        assert_eq!(
            parse_conn_query("wss://gw.example.com/ws"),
            (0, String::new())
        );
        assert_eq!(
            parse_conn_query("wss://gw.example.com/ws?a=b"),
            (0, String::new())
        );
    }

    /// app_id 形态：合规不告警，可疑只告警不拒绝
    #[test]
    fn app_id_form_check() {
        assert!(check_app_id("cli_0123456789abcdef").is_none());
        assert!(check_app_id("cli_0123456789ABCDEF").is_none());
        assert!(check_app_id("ou_0123456789abcdef").is_some());
        assert!(check_app_id("cli_short").is_some());
        assert!(check_app_id("cli_0123456789abcdeg").is_some());
    }

    /// 服务端下发参数覆盖兜底默认值（`ReconnectCount = -1` ⇒ 无限）
    #[test]
    fn params_apply_config_overrides() {
        let mut params = WsParams::default();
        assert_eq!(params.ping_interval_secs, DEFAULT_PING_INTERVAL_SECS);
        assert_eq!(params.reconnect_count, None);
        params.apply_config(&ClientConfig {
            ping_interval: Some(30),
            reconnect_interval: Some(10),
            reconnect_count: Some(5),
            reconnect_nonce: Some(3),
        });
        assert_eq!(params.ping_interval_secs, 30);
        assert_eq!(params.reconnect_interval_secs, 10);
        assert_eq!(params.reconnect_nonce_secs, 3);
        assert_eq!(params.reconnect_count, Some(5));
        // 负数（官方 -1）= 无限
        params.apply_config(&ClientConfig {
            reconnect_count: Some(-1),
            ..Default::default()
        });
        assert_eq!(params.reconnect_count, None);
        // 缺失字段保留原值
        params.apply_config(&ClientConfig::default());
        assert_eq!(params.ping_interval_secs, 30);
    }

    /// 心跳帧形状：pbbp2 控制帧 `type=ping` + `service_id`；间隔取自运行参数
    #[test]
    fn heartbeat_frame_shape() {
        let adapter = test_adapter();
        if let Ok(mut params) = adapter.params.write() {
            params.service_id = 7;
            params.ping_interval_secs = 15;
        }
        let hb = adapter.heartbeat().expect("lark uses app-layer ping");
        assert_eq!(hb.interval, Duration::from_secs(15));
        let Some(WsOutFrame::Binary(bytes)) = hb.frame else {
            panic!("expected binary ping frame");
        };
        let frame = Frame::decode(bytes.as_slice()).expect("decode ping");
        assert!(frame.is_control());
        assert_eq!(frame.msg_type(), Some(msg_type::PING));
        assert_eq!(frame.service, 7);

        // 服务端下发 0 ⇒ 关闭心跳
        if let Ok(mut params) = adapter.params.write() {
            params.ping_interval_secs = 0;
        }
        assert!(adapter.heartbeat().is_none());
    }

    /// 重连策略：固定间隔 + 首次抖动 + 次数上限；间隔下限 1s（防忙重连）
    #[test]
    fn reconnect_policy_from_params() {
        let adapter = test_adapter();
        if let Ok(mut params) = adapter.params.write() {
            params.reconnect_interval_secs = 0;
            params.reconnect_nonce_secs = 30;
            params.reconnect_count = Some(3);
        }
        assert_eq!(
            adapter.reconnect_policy(),
            ReconnectPolicy::Fixed {
                interval: Duration::from_secs(1),
                first_jitter: Some(Duration::from_secs(30)),
                max_attempts: Some(3),
            }
        );
    }

    /// ACK 形状：复用入帧 + `biz_rt`（官方写负值）+ payload `{"code":200}`
    #[test]
    fn ack_reply_shape() {
        let adapter = test_adapter();
        let inbound = Frame {
            seq_id: 3,
            log_id: 4,
            service: 5,
            method: pbbp2::frame_type::DATA,
            headers: vec![pbbp2::Header {
                key: header_key::TYPE.to_string(),
                value: msg_type::EVENT.to_string(),
            }],
            payload_encoding: "json".to_string(),
            payload_type: "event".to_string(),
            payload: br#"{"a":1}"#.to_vec(),
            log_id_new: "lg".to_string(),
        };
        let reply = adapter.ack_reply(&inbound, 200, Instant::now());
        let Some(WsOutFrame::Binary(bytes)) = Some(reply) else {
            panic!("expected binary ack frame");
        };
        let ack = Frame::decode(bytes.as_slice()).expect("decode ack");
        assert_eq!(ack.seq_id, 3);
        assert_eq!(ack.log_id, 4);
        assert_eq!(ack.service, 5);
        assert_eq!(ack.method, pbbp2::frame_type::DATA);
        assert_eq!(ack.payload_text(), Some(r#"{"code":200}"#));
        // biz_rt 是负值（官方 `String(startTime - endTime)`）
        let biz_rt = ack
            .header(header_key::BIZ_RT)
            .expect("biz_rt header present")
            .parse::<i64>()
            .expect("biz_rt integer");
        assert!(biz_rt <= 0, "biz_rt should be non-positive, got {}", biz_rt);
    }

    /// 脏帧/文本帧/超限帧：留痕但不 panic、不回写
    #[tokio::test]
    async fn malformed_frames_are_not_fatal() {
        let adapter = test_adapter();
        // 乱码二进制
        let out = adapter
            .on_message(WsFrame::Binary(vec![0xff, 0xff, 0xff]))
            .await;
        assert_eq!(out.action, crate::pkg::ws::FrameAction::Continue);
        assert!(out.replies.is_empty());
        // 文本帧
        let out = adapter
            .on_message(WsFrame::Text("{\"type\":\"ping\"}".to_string()))
            .await;
        assert!(out.replies.is_empty());
        // 超限帧（不真正分配 8MiB 以外，只验证判定分支）
        let out = adapter
            .on_message(WsFrame::Binary(vec![0u8; MAX_FRAME_BYTES + 1]))
            .await;
        assert!(out.replies.is_empty());
    }

    /// 非订阅数据类型（如 card）：留痕且不回 ACK
    #[tokio::test]
    async fn unsupported_data_type_gets_no_ack() {
        let adapter = test_adapter();
        let frame = Frame {
            seq_id: 1,
            log_id: 1,
            service: 1,
            method: pbbp2::frame_type::DATA,
            headers: vec![pbbp2::Header {
                key: header_key::TYPE.to_string(),
                value: msg_type::CARD.to_string(),
            }],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        };
        let out = adapter
            .on_message(WsFrame::Binary(frame.encode_to_vec()))
            .await;
        assert!(out.replies.is_empty());
    }

    /// 终局原因只置一次（保留首个根因），取端点成功后清除
    #[test]
    fn terminal_reason_is_sticky_and_clearable() {
        let adapter = test_adapter();
        assert!(adapter.terminal_reason().is_none());
        adapter.set_terminal("first");
        adapter.set_terminal("second");
        assert_eq!(adapter.terminal_reason().as_deref(), Some("first"));
        if let Ok(mut t) = adapter.terminal.lock() {
            *t = None;
        }
        assert!(adapter.terminal_reason().is_none());
    }
}
