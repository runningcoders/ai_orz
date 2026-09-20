//! 微信 iLink 消息面协议客户端（DAO 层）
//!
//! 与 `pkg/wechat_ilink.rs`（配置面登录协议）分离：本文件承载 bot 令牌下的
//! 消息收发——`getupdates` 长轮询、`sendmessage` 出站，以及**受管**长轮询循环
//! （registry 管理 stop / ensure，对齐 lark WS 的 registry 管理模式）。
//!
//! 分层约束：
//! - 读循环里不做业务：收帧即 publish AOP 事件（[`WechatInboundEvent`]），
//!   由 `ConsumeMode::Async` 的 consumer 消费；
//! - 游标 / 会话写回经 [`InboundStateWriter`] 窄接口（init 时注入 message_channel DAO，
//!   测试可注入内存实现），本模块不依赖其他 DAO 的完整类型；
//! - 接入域以凭证 `base_url` 为准（登录响应带回），禁硬编码默认域；
//! - 出站客户端必须走 `pkg/http` preset，禁止裸 `reqwest::Client::new()`。

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;
use tokio::sync::RwLock;

use common::error::{Result, err};
use common::models::inbound_state::InboundState;

use crate::models::events::{IlinkMessage, WechatInboundEvent};
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;

use super::ILINK_DEFAULT_BASE_URL;

// ==================== 凭证 ====================

/// iLink 渠道运行凭证（由 DAL 层按渠道 `wechat_credential_id` 引用解析后传入，
/// DAO 不做凭证解析——与飞书 `LarkAppCredentials` 同构）
#[derive(Clone)]
pub struct IlinkChannelCredentials {
    /// bot 令牌（已解密）
    pub bot_token: String,
    /// iLink bot 标识
    pub bot_id: String,
    /// 接入域（登录 confirmed 响应带回，getupdates / sendmessage 等均以此为准）
    pub base_url: String,
}

impl std::fmt::Debug for IlinkChannelCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IlinkChannelCredentials")
            .field("bot_id", &self.bot_id)
            .field("base_url", &self.base_url)
            .field("bot_token", &"***")
            .finish()
    }
}

impl IlinkChannelCredentials {
    /// 凭证指纹（bot_id / bot_token / base_url 任一变化即不同）。
    ///
    /// 用标准哈希而非明文拼接：指纹常驻内存 registry，避免令牌以可读形式留存。
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.bot_id.hash(&mut hasher);
        self.bot_token.hash(&mut hasher);
        self.base_url.hash(&mut hasher);
        hasher.finish()
    }
}

/// 从凭证行解析 iLink 渠道凭证（纯函数，可测）
///
/// 解析规则：凭证行（已由调用方按渠道 `wechat_credential_id` 查得）→
/// 校验 kind=WechatIlink → 解密 bot_token；任一环节失败返回引导性错误。
pub fn resolve_ilink_credentials(
    credential: &crate::models::user_credential::UserCredentialPo,
    channel: &MessageChannel,
) -> Result<IlinkChannelCredentials> {
    let credential_id = credential.id.as_str();
    if credential.kind != common::models::CredentialKind::WechatIlink {
        return Err(err!(
            InvalidRequest,
            "微信渠道引用的凭证类型不匹配 channel_id={} credential_id={}",
            channel.po.id,
            credential_id
        ));
    }
    let common::models::CredentialDetail::WechatIlink {
        bot_token,
        bot_id,
        base_url,
        ..
    } = &credential.detail.0
    else {
        return Err(err!(
            InvalidRequest,
            "微信渠道引用的凭证类型不匹配 channel_id={} credential_id={}",
            channel.po.id,
            credential_id
        ));
    };
    let bot_token = crate::pkg::crypto::decrypt_channel_secret(bot_token).map_err(|e| {
        err!(
            Internal,
            "微信凭证 bot_token 解密失败 channel_id={} credential_id={}: {}",
            channel.po.id,
            credential_id,
            e
        )
    })?;
    if bot_token.is_empty() || bot_id.is_empty() {
        return Err(err!(
            InvalidRequest,
            "微信凭证缺少 bot_token / bot_id channel_id={} credential_id={}，请重新扫码授权",
            channel.po.id,
            credential_id
        ));
    }
    Ok(IlinkChannelCredentials {
        bot_token,
        bot_id: bot_id.clone(),
        // base_url 原则上登录时必回填；空值宽容回落默认域（历史数据兜底）
        base_url: if base_url.is_empty() {
            ILINK_DEFAULT_BASE_URL.to_string()
        } else {
            base_url.clone()
        },
    })
}

// ==================== HTTP 协议客户端 ====================

/// 长轮询单次调用超时：服务端 hold ~35s，客户端必须大于它
pub(crate) const UPDATES_POLL_TIMEOUT_MS: u64 = 45_000;

/// getupdates 长轮询响应（游标 + 消息列表）
#[derive(Debug, Clone, Default)]
pub struct IlinkUpdates {
    /// 新游标（`get_updates_buf`；服务端未返回时为 None，保持旧游标）
    pub cursor: Option<String>,
    pub messages: Vec<IlinkMessage>,
    /// 本轮是否因**客户端超时**返回（非服务端 hold 到期）
    ///
    /// 客户端超时 45s > 服务端 hold ~35s，正常轮询**永不**触发 —— 一旦为真即
    /// 网络 hang 或服务端异常。此前该分支直接返回空批次，导致「网络断了」与
    /// 「队列就是空的」在日志上完全同形、无法区分。
    pub client_timeout: bool,
}

/// 共享客户端：getupdates 长轮询专用（45s > 服务端 hold 35s）
fn poll_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        crate::pkg::http::presets::with_timeout_ms(UPDATES_POLL_TIMEOUT_MS)
            .and_then(|opts| opts.build())
            .expect("构建 iLink 长轮询客户端失败")
    })
}

/// 共享客户端：普通调用（sendmessage 等，30s）
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        crate::pkg::http::presets::outbound()
            .build()
            .expect("构建 iLink HTTP 客户端失败")
    })
}

/// iLink 请求头三件套：AuthorizationType + Bearer token + X-WECHAT-UIN（随机 uint32 base64）
fn auth_headers(builder: reqwest::RequestBuilder, bot_token: &str) -> reqwest::RequestBuilder {
    let uin = rand::random::<u32>().to_le_bytes();
    builder
        .header("AuthorizationType", "ilink_bot_token")
        .header("Authorization", format!("Bearer {bot_token}"))
        .header("X-WECHAT-UIN", BASE64.encode(uin))
}

fn http_err(op: &str, e: reqwest::Error) -> common::error::Error {
    err!(ThirdPartyError, "ilink {} http error: {}", op, e)
}

/// 日志用令牌摘要（游标 / 消息键等 opaque 值可能很长）
///
/// 只留前 8 个**字符**（按 char 而非字节切，避免切坏多字节）并附原始长度：
/// 既够在日志里对照"是不是同一个值"，又不把整个长串灌进日志。
pub(crate) fn short_token(value: &str) -> String {
    let mut chars = value.chars();
    let head: String = chars.by_ref().take(8).collect();
    if chars.next().is_some() {
        format!("{head}…({})", value.chars().count())
    } else {
        head
    }
}

/// 收帧日志用的消息键摘要（最多 [`INBOUND_LOG_KEY_LIMIT`] 个，超出只报数量）
///
/// 键缺失时显示 `<auto>`（循环会为它生成占位 ID），便于对照"服务端没给幂等键"。
fn brief_message_keys(messages: &[IlinkMessage]) -> String {
    let mut keys: Vec<String> = messages
        .iter()
        .take(INBOUND_LOG_KEY_LIMIT)
        .map(|m| {
            let key = m.message_key();
            if key.is_empty() {
                "<auto>".to_string()
            } else {
                short_token(&key)
            }
        })
        .collect();
    if messages.len() > INBOUND_LOG_KEY_LIMIT {
        keys.push(format!("+{}", messages.len() - INBOUND_LOG_KEY_LIMIT));
    }
    keys.join(", ")
}

/// 拉取增量消息（长轮询单次调用；客户端超时视为本轮无事件——服务端 hold 常态）
pub async fn get_updates(
    credentials: &IlinkChannelCredentials,
    cursor: Option<&str>,
) -> Result<IlinkUpdates> {
    let url = format!("{}/ilink/bot/getupdates", credentials.base_url);
    let body = serde_json::json!({ "get_updates_buf": cursor.unwrap_or_default() });
    let resp = auth_headers(poll_client().post(&url), &credentials.bot_token)
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        // 客户端超时：标记后交回循环记 warn（见 `IlinkUpdates::client_timeout`）。
        // 不在此处记日志：DAO 协议层无 channel_id，且循环侧才掌握"第几次/连续几次"。
        Err(e) if e.is_timeout() => {
            return Ok(IlinkUpdates {
                client_timeout: true,
                ..Default::default()
            });
        }
        Err(e) => return Err(http_err("getupdates", e)),
    };
    let text = match resp.error_for_status() {
        Ok(r) => r.text().await.map_err(|e| http_err("getupdates", e))?,
        Err(e) if e.is_timeout() => return Ok(IlinkUpdates::default()),
        Err(e) => return Err(http_err("getupdates", e)),
    };
    parse_updates(&text)
}

/// 解析 getupdates 响应（抽纯函数便于单测）
///
/// 兼容 `msg_list` / `msgs` 两种列表字段名（协议较新，宽容解析）；
/// 单条消息解析失败跳过（不因脏数据中断整批）。
fn parse_updates(body: &str) -> Result<IlinkUpdates> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| err!(ThirdPartyError, "ilink getupdates 响应非 JSON: {}", e))?;
    let cursor = value
        .get("get_updates_buf")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let raw_messages = value
        .get("msg_list")
        .or_else(|| value.get("msgs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let messages = raw_messages
        .into_iter()
        .filter_map(|m| serde_json::from_value::<IlinkMessage>(m).ok())
        .collect();
    Ok(IlinkUpdates {
        cursor,
        messages,
        // 能解析出响应体 = 服务端已返回，非客户端超时
        ..Default::default()
    })
}

/// 构造 sendmessage 请求体（抽纯函数便于单测）
///
/// 规则（设计文档 §5.1）：`from_user_id` 留空、`to_user_id` 填对端、
/// `context_token` 回传收到的最新值、文本走 item_list 文本条目。
fn build_send_body(to_user_id: &str, context_token: &str, text: &str, client_id: &str) -> Value {
    serde_json::json!({
        "msg": {
            "from_user_id": "",
            "to_user_id": to_user_id,
            "client_id": client_id,
            "message_type": "BOT",
            "message_state": "FINISH",
            "item_list": [
                { "type": 1, "text_item": { "content": text } }
            ],
            "context_token": context_token,
        }
    })
}

/// 发送文本消息到对端（出站）
///
/// 返回服务端 client_id（响应缺省时回传本地生成的占位值）。
pub async fn send_text(
    credentials: &IlinkChannelCredentials,
    to_user_id: &str,
    context_token: &str,
    text: &str,
) -> Result<()> {
    if to_user_id.is_empty() {
        return Err(err!(
            InvalidRequest,
            "iLink 发送缺少对端标识 peer_id（渠道从未收到入站消息，请先在微信里发一条消息）"
        ));
    }
    if context_token.is_empty() {
        return Err(err!(
            InvalidRequest,
            "iLink 发送缺少 context_token（会话令牌滚动刷新，请让对端先发一条消息再回复）"
        ));
    }
    let url = format!("{}/ilink/bot/sendmessage", credentials.base_url);
    let client_id = uuid::Uuid::now_v7().to_string();
    let body = build_send_body(to_user_id, context_token, text, &client_id);
    let resp = auth_headers(client().post(&url), &credentials.bot_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| http_err("sendmessage", e))?;
    let resp = resp
        .error_for_status()
        .map_err(|e| http_err("sendmessage", e))?;
    let text = resp.text().await.map_err(|e| http_err("sendmessage", e))?;
    check_send_response(&text)
}

/// 校验 sendmessage 响应（协议含 ret 错误码时非 0 报错；抽纯函数便于单测）
fn check_send_response(body: &str) -> Result<()> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        // 响应非 JSON：HTTP 2xx 已通过，宽容视为成功（协议较新）
        return Ok(());
    };
    let ret = value.get("ret").and_then(Value::as_i64).unwrap_or(0);
    if ret != 0 {
        let msg = value
            .get("errmsg")
            .or_else(|| value.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(err!(
            ThirdPartyError,
            "ilink sendmessage 失败: ret={} msg={}",
            ret,
            msg
        ));
    }
    Ok(())
}

// ==================== 入站运行状态写回 ====================

/// 入站运行状态写回窄接口（轮询循环独占写 `message_channels.inbound_state` 列）
///
/// DAO 不直接依赖 MessageChannelDao 完整类型（DAO 不依赖其他 DAO）：
/// init 时注入薄实现，测试可注入内存实现。
#[async_trait::async_trait]
pub trait InboundStateWriter: Send + Sync {
    /// 整列覆盖写（失败仅告警，不中断轮询——运行态丢失等价从头拉取）
    async fn save(&self, channel_id: &str, state: &InboundState);
}

/// 生产实现：委托 MessageChannelDao::set_inbound_state
pub(crate) struct MessageChannelStateWriter {
    dao: Arc<dyn crate::service::dao::message_channel::MessageChannelDao>,
}

impl MessageChannelStateWriter {
    pub fn new(dao: Arc<dyn crate::service::dao::message_channel::MessageChannelDao>) -> Self {
        Self { dao }
    }
}

#[async_trait::async_trait]
impl InboundStateWriter for MessageChannelStateWriter {
    async fn save(&self, channel_id: &str, state: &InboundState) {
        let ctx = RequestContext::new_system();
        if let Err(e) = self
            .dao
            .set_inbound_state(ctx, channel_id, &state.to_json())
            .await
        {
            log_warn!(
                "ilink inbound_state 写回失败（忽略）: channel_id={} err={}",
                channel_id,
                e
            );
        }
    }
}

// ==================== 已确认游标（P2 的落点）====================

/// 已确认消费的入站游标（channel_id → 服务端 **opaque** 值）
///
/// **改造前**（P2 缺陷）：轮询循环在 `publish` 之后**无条件**推进游标
/// （"服务端返回新值就覆盖"）—— 事件还没被消费，游标已经越过它。
/// 一旦消费失败，同一批消息再也不会被重新拉到 → **消息确定性丢失**，
/// 且因为不报错而极难察觉（去重 ≠ 重放）。
///
/// **现在**：
/// - 轮询循环**只读**本表（用已确认值作为 `getupdates` 的请求游标）；
/// - 服务端返回的新游标随事件带出（[`WechatInboundEvent::cursor`]）；
/// - 消费侧确认（`WechatDalImpl` 的 `on_consumed` → `WechatDao::advance_inbound_cursor`）
///   才把新值写入本表。
///
/// 于是「上一轮没消费完 → 游标不动 → 下一轮重拉同一批」：重复由 `message_key`
/// 幂等去重吸收，代价是少量重复拉取，换来「失败不丢消息」。
#[derive(Default)]
pub(crate) struct CursorStore {
    values: RwLock<HashMap<String, String>>,
}

impl CursorStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 已确认的游标（`None` = 从未确认过 → 从头拉，由幂等键兜底重复）
    pub async fn get(&self, channel_id: &str) -> Option<String> {
        self.values
            .read()
            .await
            .get(channel_id)
            .cloned()
            .filter(|v| !v.is_empty())
    }

    /// 确认推进（仅由消费侧回调触发；opaque 值只能"后来的覆盖先前的"）
    pub async fn set(&self, channel_id: &str, value: &str) {
        if value.is_empty() {
            return;
        }
        self.values
            .write()
            .await
            .insert(channel_id.to_string(), value.to_string());
    }
}

// ==================== 受管长轮询循环 ====================

/// 轮询循环句柄：任务 + 启动时凭证指纹（ensure 幂等 / 凭证变化自动重建）
struct PollLoopHandle {
    join: tokio::task::JoinHandle<()>,
    fingerprint: u64,
    /// 运行态快照（监控读取入口，与飞书 `WsClientState.conn_state` 同构）
    stats: Arc<RwLock<PollRuntimeStats>>,
}

/// 长轮询运行态快照（监控用）
///
/// 由 `poll_loop` 独占写、[`PollLoopRegistry::listener_stats`] 读。
///
/// **判活不能只看「句柄在注册表里」**：任务可能已 panic / 卡死，而句柄仍在。
/// 真判据是 `rounds` 与 `last_poll_at_ms` 是否推进——正常时长轮询约 35s 一轮。
#[derive(Debug, Clone)]
struct PollRuntimeStats {
    /// 渠道名称（建连时从渠道行取，仅展示用）
    channel_name: String,
    /// iLink bot 标识
    bot_id: String,
    /// 累计完成轮次（每轮成功返回 +1，含超时宽容的空批次）
    rounds: u64,
    /// 累计入站消息数
    inbound_messages: u64,
    /// 连续失败次数（成功一轮归零）
    consecutive_failures: u32,
    /// 累计客户端超时次数
    client_timeouts: u64,
    /// 最近一次**正常**轮询返回时间（ms；客户端超时不刷新，便于与网络 hang 区分）
    last_poll_at_ms: i64,
    /// 最近一条入站消息时间（ms；0 = 从未收到）
    last_message_at_ms: i64,
}

impl PollRuntimeStats {
    fn new(channel_name: String, bot_id: String) -> Self {
        Self {
            channel_name,
            bot_id,
            rounds: 0,
            inbound_messages: 0,
            consecutive_failures: 0,
            client_timeouts: 0,
            last_poll_at_ms: 0,
            last_message_at_ms: 0,
        }
    }

    /// 轮询阶段：连续失败即 `degraded`（退避中），否则 `polling`
    fn state(&self) -> &'static str {
        if self.consecutive_failures > 0 {
            "degraded"
        } else {
            "polling"
        }
    }
}

/// 连续失败重试节奏：前 5 次间隔 2s，超过后退避 30s（避免触发限流）
const FAIL_RETRY_FAST_MS: u64 = 2_000;
const FAIL_RETRY_SLOW_MS: u64 = 30_000;
const FAIL_FAST_LIMIT: u32 = 5;
/// 正常轮询间隙（长轮询本身 hold 35s，小幅间隔防紧密打转）
const POLL_PAUSE_MS: u64 = 500;
/// 心跳日志间隔（毫秒）：正常轮询（尤其空轮询）此前**零日志**，无法从日志
/// 判断「监听是否在跑」。5 分钟 ≈ 8 轮长轮询，既能判活又不刷屏。
const HEARTBEAT_INTERVAL_MS: i64 = 300_000;
/// 收帧日志里最多列出的消息键个数（超出只报数量）
const INBOUND_LOG_KEY_LIMIT: usize = 3;

/// 长轮询循环体：收帧 publish 事件（带本轮游标）+ 刷会话 + 一次写回
///
/// **游标不由本循环推进**（P2）：循环只读 [`CursorStore`] 的已确认值，
/// 服务端返回的新游标随事件交出去，等消费侧确认后回调 `advance_inbound_cursor`
/// 才前进 —— 上一轮没消费完时游标不动，下一轮重拉同一批（幂等键兜底）。
///
/// 终止方式：registry 移除句柄时 `abort()`。单 writer 独占 `inbound_state`，
/// abort 只可能损失"最后一轮"的状态写回，游标回退由事件幂等键兜底。
async fn poll_loop(
    channel_id: String,
    credentials: IlinkChannelCredentials,
    mut state: InboundState,
    writer: Option<Arc<dyn InboundStateWriter>>,
    cursors: Arc<CursorStore>,
    stats: Arc<RwLock<PollRuntimeStats>>,
) {
    let resuming = cursors.get(&channel_id).await;
    log_info!(
        "ilink poll loop started: channel_id={} bot_id={} base_url={} resume_cursor={}",
        channel_id,
        credentials.bot_id,
        credentials.base_url,
        resuming
            .as_deref()
            .map(short_token)
            .unwrap_or_else(|| "<none>".to_string())
    );

    // 心跳基准（仅日志用；其余计数一律以 `stats` 为唯一存储，避免双份漂移）
    let started_at_ms = common::constants::utils::current_timestamp_ms();
    let mut last_heartbeat_ms = started_at_ms;

    loop {
        // 请求游标 = **已确认**消费的游标（P2）：上一轮没消费完则不动，下一轮重拉
        let cursor = cursors.get(&channel_id).await;
        match get_updates(&credentials, cursor.as_deref()).await {
            Err(e) => {
                // 失败计入运行态快照：连续失败 > 0 → 前端「降级中」阶段
                let failures = {
                    let mut s = stats.write().await;
                    s.consecutive_failures = s.consecutive_failures.saturating_add(1);
                    s.consecutive_failures
                };
                let delay = if failures <= FAIL_FAST_LIMIT {
                    FAIL_RETRY_FAST_MS
                } else {
                    FAIL_RETRY_SLOW_MS
                };
                log_warn!(
                    "ilink getupdates failed (retry in {}ms): channel_id={} failures={} err={}",
                    delay,
                    channel_id,
                    failures,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            Ok(updates) => {
                let IlinkUpdates {
                    cursor: new_cursor,
                    messages,
                    client_timeout,
                } = updates;
                let message_count = messages.len();
                let now_ms = common::constants::utils::current_timestamp_ms();

                // 运行态快照一次性更新（`stats` 是唯一存储：心跳日志与监控 API 同源，
                // 避免两套计数各自漂移）
                let total_client_timeouts = {
                    let mut s = stats.write().await;
                    s.rounds = s.rounds.saturating_add(1);
                    s.consecutive_failures = 0;
                    if client_timeout {
                        s.client_timeouts = s.client_timeouts.saturating_add(1);
                    } else {
                        // 仅正常返回刷新：客户端超时不刷新 → 前端可区分「网络 hang」
                        // （超时数增长但 last_poll 不动）与「整循环卡死」（两者都不动）
                        s.last_poll_at_ms = now_ms;
                    }
                    if message_count > 0 {
                        s.inbound_messages =
                            s.inbound_messages.saturating_add(message_count as u64);
                        s.last_message_at_ms = now_ms;
                    }
                    s.client_timeouts
                };

                // 客户端超时（正常永不触发）：与"服务端 hold 到期返回空批次"分开记，
                // 否则「网络 hang」与「队列本就是空的」在日志上完全同形。
                if client_timeout {
                    log_warn!(
                        "ilink getupdates client timeout ({}ms; server holds ~35s → abnormal): channel_id={} timeouts={}",
                        UPDATES_POLL_TIMEOUT_MS,
                        channel_id,
                        total_client_timeouts
                    );
                }

                // 收帧日志：此前"收到消息"没有任何日志，链路是否有消息只能靠下游倒推
                if message_count > 0 {
                    log_info!(
                        "ilink inbound batch: channel_id={} bot_id={} count={} keys=[{}] new_cursor={}",
                        channel_id,
                        credentials.bot_id,
                        message_count,
                        brief_message_keys(&messages),
                        new_cursor
                            .as_deref()
                            .map(short_token)
                            .unwrap_or_else(|| "<none>".to_string())
                    );
                }

                // 收帧即 publish（入队即返回），业务由 Async consumer 消费
                for message in messages {
                    let message_key = {
                        let key = message.message_key();
                        if key.is_empty() {
                            uuid::Uuid::now_v7().to_string()
                        } else {
                            key
                        }
                    };
                    // 刷会话：入站即覆盖写 context_token（滚动刷新的动态令牌）
                    state.sessions.upsert(
                        message.from_user_id.clone(),
                        message.context_token.clone(),
                        Some(message_key.clone()),
                        now_ms,
                    );
                    state.sessions.retain_default();
                    let event = WechatInboundEvent {
                        channel_id: channel_id.clone(),
                        bot_id: credentials.bot_id.clone(),
                        message_key,
                        // 本轮游标随事件带出：消费确认后才由 `on_consumed` 推进（P2）
                        cursor: new_cursor.clone(),
                        message,
                    };
                    crate::pkg::aop::registry()
                        .publish(&RequestContext::new_system(), event)
                        .await;
                }

                // 无消息轮次：没有待确认的事件 → 游标可直接推进
                // （否则服务端在空轮次里给出的新游标永远推不动，长轮询停在原位）
                if message_count == 0
                    && let Some(cv) = new_cursor.as_deref()
                {
                    cursors.set(&channel_id, cv).await;
                }

                // ⚠️ **有消息时不在本循环推进游标**（P2）：新游标已随事件交给消费侧，
                // 由 `on_consumed` → `advance_inbound_cursor` 确认后才前进。
                // 上一轮没消费完 → 游标不动 → 下一轮重拉同一批（幂等键吸收重复）。

                // 落库前把已确认游标同步进 state（供下次启动基线；未推进则保持原值）
                if let Some(confirmed) = cursors.get(&channel_id).await {
                    state.cursor = Some(common::models::inbound_state::InboundCursor::opaque(
                        confirmed, "ilink",
                    ));
                    if let Some(c) = state.cursor.as_mut() {
                        c.updated_at_ms = Some(now_ms);
                    }
                }

                // 一次写回：会话合并（游标为已确认值）；空轮询零写入
                if (message_count > 0 || new_cursor.is_some())
                    && let Some(writer) = &writer
                {
                    writer.save(&channel_id, &state).await;
                }
                tokio::time::sleep(std::time::Duration::from_millis(POLL_PAUSE_MS)).await;

                // 心跳：轮次 / 累计入站 / 连续失败 / 客户端超时 / 游标 / 空闲时长。
                // 有这一行才可能"从日志判断监听在不在跑"——此前只能靠抓 TCP 连接佐证。
                if now_ms.saturating_sub(last_heartbeat_ms) >= HEARTBEAT_INTERVAL_MS {
                    last_heartbeat_ms = now_ms;
                    let confirmed = cursors.get(&channel_id).await;
                    let s = stats.read().await;
                    // 空闲基准：收到过消息则从最近一条算起，否则从循环启动算起
                    let idle_base = if s.last_message_at_ms > 0 {
                        s.last_message_at_ms
                    } else {
                        started_at_ms
                    };
                    log_info!(
                        "ilink poll heartbeat: channel_id={} rounds={} inbound_messages={} consecutive_failures={} client_timeouts={} cursor={} idle_ms={}",
                        channel_id,
                        s.rounds,
                        s.inbound_messages,
                        s.consecutive_failures,
                        s.client_timeouts,
                        confirmed
                            .as_deref()
                            .map(short_token)
                            .unwrap_or_else(|| "<none>".to_string()),
                        now_ms.saturating_sub(idle_base)
                    );
                }
            }
        }
    }
}

/// 受管轮询 registry：channel_id 键控（阶段一：一个 bot 微信号 = 一个 channel）
pub(crate) struct PollLoopRegistry {
    loops: RwLock<HashMap<String, PollLoopHandle>>,
}

impl PollLoopRegistry {
    pub fn new() -> Self {
        Self {
            loops: RwLock::new(HashMap::new()),
        }
    }

    /// 确保 channel 的轮询循环以指定凭证运行（幂等）
    ///
    /// - 未运行 → 启动；
    /// - 运行中且凭证指纹相同 → no-op（幂等）；
    /// - 运行中但指纹不同（bot_id / bot_token / base_url 任一变化）→ 停旧重建。
    pub async fn ensure(
        &self,
        channel: &MessageChannel,
        credentials: &IlinkChannelCredentials,
        writer: Option<Arc<dyn InboundStateWriter>>,
        cursors: Arc<CursorStore>,
    ) -> Result<()> {
        let fingerprint = credentials.fingerprint();
        {
            let loops = self.loops.read().await;
            if let Some(handle) = loops.get(channel.id())
                && handle.fingerprint == fingerprint
            {
                return Ok(());
            }
        }
        // 指纹不同或未运行：先移除旧句柄（abort 旧任务），再启动新循环
        let removed = self.stop(channel.id()).await;

        // 入站运行状态：从渠道行加载（from_json 解析失败 = 无状态，从头开始，fail-open）
        let state = channel
            .po
            .inbound_state
            .as_deref()
            .and_then(InboundState::from_json)
            .unwrap_or_default();

        // 游标回灌（§5.6「进程重启从上次游标续拉」的实现口径）：
        // `CursorStore` 是**进程内**的，重启后为空；落库的 `inbound_state.cursor`
        // 是已确认消费的进度，也是重启后唯一的进度来源。
        // 不回灌 → 首轮以空游标请求，而 iLink 对空游标**不重放**历史（实测 `msgs: []`）
        // → 停机期间的消息永久丢失，且无日志、无报错（因此极难察觉）。
        if let Some(cursor) = state.cursor.as_ref().filter(|c| !c.is_empty()) {
            cursors.set(channel.id(), &cursor.value).await;
            log_info!(
                "ilink inbound cursor restored from inbound_state: channel_id={} cursor={} updated_at_ms={:?}",
                channel.id(),
                short_token(&cursor.value),
                cursor.updated_at_ms
            );
        }

        let channel_id = channel.id().to_string();
        let credentials = credentials.clone();
        // 运行态快照：建连时快照一次渠道名与 bot_id（展示用；改名需重建才刷新，可接受）
        let stats = Arc::new(RwLock::new(PollRuntimeStats::new(
            channel.po.channel_name.clone(),
            credentials.bot_id.clone(),
        )));
        let join = tokio::spawn(poll_loop(
            channel_id.clone(),
            credentials,
            state,
            writer,
            cursors,
            stats.clone(),
        ));
        self.loops.write().await.insert(
            channel_id.clone(),
            PollLoopHandle {
                join,
                fingerprint,
                stats,
            },
        );
        if removed {
            log_info!(
                "ilink poll loop rebuilt (credentials changed): channel_id={}",
                channel_id
            );
        }
        Ok(())
    }

    /// 停止指定 channel 的轮询循环（未运行时幂等返回 false）
    pub async fn stop(&self, channel_id: &str) -> bool {
        let handle = self.loops.write().await.remove(channel_id);
        match handle {
            Some(handle) => {
                handle.join.abort();
                log_info!("ilink poll loop stopped: channel_id={}", channel_id);
                true
            }
            None => false,
        }
    }

    /// 停止全部循环（优雅退出）
    pub async fn stop_all(&self) {
        let handles: Vec<(String, PollLoopHandle)> = self.loops.write().await.drain().collect();
        let stopped = handles.len();
        for (_, handle) in handles {
            handle.join.abort();
        }
        if stopped > 0 {
            log_info!("ilink poll loops stopped, total={}", stopped);
        }
    }

    /// 指定 channel 是否正在轮询
    pub async fn is_running(&self, channel_id: &str) -> bool {
        self.loops.read().await.contains_key(channel_id)
    }

    /// 全部渠道的轮询运行态快照（监控 API 用，已组装为共享 DTO）
    ///
    /// ⚠️ [`Self::is_running`] 为真只说明**句柄在册**，不代表循环真的在推进
    /// （任务可能已 panic 或卡死在 hold 里）。判活请看 `rounds` 与 `last_poll_at_ms`
    /// 是否单调前进——这也是本快照存在的原因。
    ///
    /// 游标取 `cursors` 里**已确认消费**的值（非服务端最新值），与落库口径一致。
    pub async fn metrics(
        &self,
        cursors: &CursorStore,
    ) -> Vec<common::api::WechatPollChannelMetrics> {
        let loops = self.loops.read().await;
        let mut out = Vec::with_capacity(loops.len());
        for (channel_id, handle) in loops.iter() {
            let s = handle.stats.read().await;
            let cursor = cursors.get(channel_id).await.map(|c| short_token(&c));
            out.push(common::api::WechatPollChannelMetrics {
                channel_id: channel_id.clone(),
                channel_name: s.channel_name.clone(),
                bot_id: s.bot_id.clone(),
                state: s.state().to_string(),
                rounds: s.rounds,
                inbound_messages: s.inbound_messages,
                consecutive_failures: s.consecutive_failures,
                client_timeouts: s.client_timeouts,
                last_poll_at_ms: s.last_poll_at_ms,
                last_message_at_ms: s.last_message_at_ms,
                cursor,
            });
        }
        out
    }
}

// ==================== 单测 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::CredentialDetail;

    fn channel() -> MessageChannel {
        MessageChannel::from_po(crate::models::message_channel::MessageChannelPo::new(
            "ch_wx_1".to_string(),
            "org_1".to_string(),
            "user_1".to_string(),
            None,
            common::enums::ChannelType::Wechat,
            "我的微信".to_string(),
            None,
            None,
            None,
            Default::default(),
            "user_1".to_string(),
        ))
    }

    fn credential_row(
        bot_token: &str,
        bot_id: &str,
        base_url: &str,
    ) -> crate::models::user_credential::UserCredentialPo {
        crate::models::user_credential::UserCredentialPo::new(
            "cred_1".to_string(),
            "org_1".to_string(),
            "user_1".to_string(),
            common::models::CredentialKind::WechatIlink,
            "iLink".to_string(),
            // 明文直存：decrypt_channel_secret 对无 enc:v1: 前缀的值透传（不依赖 master_key 配置）
            CredentialDetail::WechatIlink {
                bot_token: bot_token.to_string(),
                bot_id: bot_id.to_string(),
                user_id: None,
                base_url: base_url.to_string(),
            },
            common::models::CredentialVisibility::Private,
            "user_1".to_string(),
        )
    }

    /// 凭证解析：kind 校验 + bot_token 解密 + base_url 空值回落默认域
    #[test]
    fn test_resolve_ilink_credentials() {
        let ch = channel();
        let row = credential_row("tok_plain", "bot_1", "https://alt.example.com");
        let resolved = resolve_ilink_credentials(&row, &ch).unwrap();
        assert_eq!(resolved.bot_token, "tok_plain");
        assert_eq!(resolved.bot_id, "bot_1");
        assert_eq!(resolved.base_url, "https://alt.example.com");

        // base_url 空：回落默认接入域
        let row = credential_row("tok_plain", "bot_1", "");
        assert_eq!(
            resolve_ilink_credentials(&row, &ch).unwrap().base_url,
            ILINK_DEFAULT_BASE_URL
        );

        // kind 不匹配：报错
        let mut row = credential_row("tok", "bot_1", "https://x");
        row.kind = common::models::CredentialKind::GithubToken;
        assert!(resolve_ilink_credentials(&row, &ch).is_err());
    }

    /// 凭证指纹：三要素任一变化即不同；同凭证稳定
    #[test]
    fn test_credentials_fingerprint() {
        let a = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://x".into(),
        };
        let same = a.clone();
        assert_eq!(a.fingerprint(), same.fingerprint());

        let mut b = a.clone();
        b.bot_id = "bot_2".into();
        assert_ne!(a.fingerprint(), b.fingerprint());

        let mut c = a.clone();
        c.base_url = "https://y".into();
        assert_ne!(a.fingerprint(), c.fingerprint());

        let mut d = a.clone();
        d.bot_token = "tok2".into();
        assert_ne!(a.fingerprint(), d.fingerprint());
    }

    /// getupdates 解析：游标 + msg_list；兼容 msgs 字段名；脏消息跳过
    #[test]
    fn test_parse_updates() {
        let body = r#"{
            "ret": 0,
            "get_updates_buf": "cur_abc",
            "msg_list": [
                {"from_user_id":"p1","client_id":"c1","message_type":"USER","message_state":"FINISH","context_token":"t1","item_list":[{"type":1,"text_item":{"content":"hi"}}]},
                {"item_list": 1}
            ]
        }"#;
        let updates = parse_updates(body).unwrap();
        assert_eq!(updates.cursor.as_deref(), Some("cur_abc"));
        assert_eq!(updates.messages.len(), 1);
        assert_eq!(updates.messages[0].from_user_id, "p1");
        assert_eq!(updates.messages[0].text(), Some("hi".to_string()));

        // msgs 字段名兼容
        let body = r#"{"msgs":[{"from_user_id":"p2","client_id":"c2"}]}"#;
        let updates = parse_updates(body).unwrap();
        assert_eq!(updates.cursor, None);
        assert_eq!(updates.messages.len(), 1);
        assert_eq!(updates.messages[0].message_key(), "c2");

        // 空响应（服务端 hold 到期）
        let updates = parse_updates(r#"{"ret":0}"#).unwrap();
        assert!(updates.messages.is_empty());
        assert_eq!(updates.cursor, None);
    }

    /// sendmessage 请求体：from 留空 / to 填对端 / context_token 回传
    #[test]
    fn test_build_send_body() {
        let body = build_send_body("peer_1", "ctx_tok", "回复内容", "cid_local");
        let msg = body.get("msg").unwrap();
        assert_eq!(msg.get("from_user_id").and_then(Value::as_str), Some(""));
        assert_eq!(
            msg.get("to_user_id").and_then(Value::as_str),
            Some("peer_1")
        );
        assert_eq!(
            msg.get("context_token").and_then(Value::as_str),
            Some("ctx_tok")
        );
        assert_eq!(msg.get("message_type").and_then(Value::as_str), Some("BOT"));
        assert_eq!(
            msg.get("message_state").and_then(Value::as_str),
            Some("FINISH")
        );
        let items = msg.get("item_list").and_then(Value::as_array).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0]
                .get("text_item")
                .unwrap()
                .get("content")
                .and_then(Value::as_str),
            Some("回复内容")
        );
    }

    /// sendmessage 响应校验：ret=0 / 非 JSON 宽容通过 / ret!=0 报错
    #[test]
    fn test_check_send_response() {
        assert!(check_send_response(r#"{"ret":0}"#).is_ok());
        assert!(check_send_response("ok").is_ok());
        let e = check_send_response(r#"{"ret":1001,"errmsg":"token expired"}"#).unwrap_err();
        assert!(e.to_string().contains("1001"));
    }

    /// registry：ensure 幂等（同指纹 no-op）/ 指纹变化重建 / stop 幂等
    #[tokio::test]
    async fn test_poll_loop_registry_lifecycle() {
        let registry = PollLoopRegistry::new();
        let ch = channel();
        let creds = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://invalid.test".into(),
        };

        registry
            .ensure(&ch, &creds, None, Arc::new(CursorStore::new()))
            .await
            .unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 同指纹：幂等（不重建）
        registry
            .ensure(&ch, &creds, None, Arc::new(CursorStore::new()))
            .await
            .unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 指纹变化：重建（stop + start）
        let mut creds2 = creds.clone();
        creds2.bot_token = "tok2".into();
        registry
            .ensure(&ch, &creds2, None, Arc::new(CursorStore::new()))
            .await
            .unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // stop 幂等
        assert!(registry.stop("ch_wx_1").await);
        assert!(!registry.is_running("ch_wx_1").await);
        assert!(!registry.stop("ch_wx_1").await);

        registry.stop_all().await;
        assert!(!registry.is_running("ch_wx_1").await);
    }

    /// 轮询阶段判定：连续失败 > 0 → degraded，归零 → polling
    #[test]
    fn test_poll_runtime_stats_state() {
        let mut s = PollRuntimeStats::new("我的微信".to_string(), "bot_1".to_string());
        assert_eq!(s.state(), "polling");
        s.consecutive_failures = 1;
        assert_eq!(s.state(), "degraded");
        s.consecutive_failures = 0;
        assert_eq!(s.state(), "polling");
    }

    /// 运行态快照：无监听 → 空；ensure 后按 channel 暴露建连时快照的展示字段
    ///
    /// 注意此处**不断言** `state` / `last_poll_at_ms`：循环会真的去请求
    /// `invalid.test` 并失败，断言初始值会 flaky（阶段判定由上面的纯单测覆盖）。
    #[tokio::test]
    async fn test_poll_registry_metrics_snapshot() {
        let registry = PollLoopRegistry::new();
        let cursors = Arc::new(CursorStore::new());
        assert!(registry.metrics(&cursors).await.is_empty());

        let ch = channel();
        let creds = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://invalid.test".into(),
        };
        registry
            .ensure(&ch, &creds, None, cursors.clone())
            .await
            .unwrap();

        let metrics = registry.metrics(&cursors).await;
        assert_eq!(metrics.len(), 1);
        let m = &metrics[0];
        assert_eq!(m.channel_id, "ch_wx_1");
        assert_eq!(m.bot_id, "bot_1");
        // 渠道名在建连时快照（`channel()` 构造为「我的微信」）
        assert_eq!(m.channel_name, "我的微信");
        // 尚无已确认消费 → cursor 为 None（而非空串占位）
        assert_eq!(m.cursor, None);

        // 句柄移除后快照同步清空（与 is_running 同源，不残留幽灵行）
        registry.stop_all().await;
        assert!(registry.metrics(&cursors).await.is_empty());
    }

    /// 监控快照里的游标是**已确认消费**值，且以摘要形式透出（不灌全量 opaque 串）
    #[tokio::test]
    async fn test_poll_metrics_cursor_is_confirmed_and_shortened() {
        let registry = PollLoopRegistry::new();
        let cursors = Arc::new(CursorStore::new());
        let ch = channel();
        let creds = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://invalid.test".into(),
        };
        registry
            .ensure(&ch, &creds, None, cursors.clone())
            .await
            .unwrap();

        // 消费确认推进游标（模拟 on_consumed 回调）后，快照应反映该值
        cursors.set("ch_wx_1", "abcdefghij").await;
        let metrics = registry.metrics(&cursors).await;
        assert_eq!(metrics[0].cursor.as_deref(), Some("abcdefgh…(10)"));

        registry.stop_all().await;
    }

    /// P2：已确认游标存储 —— 覆盖语义（opaque 不可比较）/ 空值忽略 / channel 隔离
    #[tokio::test]
    async fn test_cursor_store_semantics() {
        let store = CursorStore::new();
        assert_eq!(store.get("ch_a").await, None);

        store.set("ch_a", "cur_1").await;
        assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_1"));

        // opaque 值只能"后来的覆盖先前的"（不可比较大小）
        store.set("ch_a", "cur_2").await;
        assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_2"));

        // 空值忽略：服务端未给出新位置 → 保持原位（否则会退回从头拉）
        store.set("ch_a", "").await;
        assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_2"));

        // channel 隔离
        assert_eq!(store.get("ch_b").await, None);
    }

    /// 日志摘要：短串原样、长串截断附长度、多字节不切坏
    #[test]
    fn test_short_token() {
        assert_eq!(short_token(""), "");
        assert_eq!(short_token("abc"), "abc");
        assert_eq!(short_token("12345678"), "12345678");
        assert_eq!(short_token("123456789"), "12345678…(9)");
        // 按 char 切（中文游标不会出现半个字符）：前 8 个字符 = 游标一二三四五六
        assert_eq!(
            short_token("游标一二三四五六七八九十"),
            "游标一二三四五六…(12)"
        );
    }

    /// 收帧键摘要：缺失键标 `<auto>`，超出上限只报数量
    #[test]
    fn test_brief_message_keys() {
        let mk = |key: &str| IlinkMessage {
            client_id: key.to_string(),
            ..Default::default()
        };
        assert_eq!(brief_message_keys(&[]), "");
        assert_eq!(brief_message_keys(&[mk("c1"), mk("c2")]), "c1, c2");
        // 无 client_id / msg_id → 循环会生成占位 ID，日志里如实标记
        assert_eq!(brief_message_keys(&[IlinkMessage::default()]), "<auto>");
        assert_eq!(
            brief_message_keys(&[mk("c1"), mk("c2"), mk("c3"), mk("c4")]),
            "c1, c2, c3, +1"
        );
    }

    /// 游标回灌（§5.6）：落库的已确认游标在 ensure 时载入内存 CursorStore
    ///
    /// 不回灌 → 重启首轮以空游标请求，而 iLink 空游标不重放历史 → 停机期间消息丢失。
    #[tokio::test]
    async fn test_ensure_restores_persisted_cursor() {
        let registry = PollLoopRegistry::new();
        let mut ch = channel();
        ch.po.inbound_state = Some(
            InboundState {
                cursor: Some(common::models::inbound_state::InboundCursor::opaque(
                    "cur_persisted",
                    "ilink",
                )),
                ..Default::default()
            }
            .to_json(),
        );
        let creds = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://invalid.test".into(),
        };
        let cursors = Arc::new(CursorStore::new());
        registry
            .ensure(&ch, &creds, None, Arc::clone(&cursors))
            .await
            .unwrap();
        assert_eq!(
            cursors.get("ch_wx_1").await.as_deref(),
            Some("cur_persisted")
        );
        registry.stop_all().await;
    }

    /// 无落库游标（或空游标）→ 不回灌，保持"从头拉、由幂等键兜底"的原语义
    #[tokio::test]
    async fn test_ensure_without_persisted_cursor_keeps_empty() {
        let creds = IlinkChannelCredentials {
            bot_token: "tok".into(),
            bot_id: "bot_1".into(),
            base_url: "https://invalid.test".into(),
        };

        // 全无 inbound_state
        let registry = PollLoopRegistry::new();
        let cursors = Arc::new(CursorStore::new());
        registry
            .ensure(&channel(), &creds, None, Arc::clone(&cursors))
            .await
            .unwrap();
        assert_eq!(cursors.get("ch_wx_1").await, None);
        registry.stop_all().await;

        // 有状态列但游标为空串
        let registry = PollLoopRegistry::new();
        let mut ch = channel();
        ch.po.inbound_state = Some(
            InboundState {
                cursor: Some(common::models::inbound_state::InboundCursor::opaque(
                    "", "ilink",
                )),
                ..Default::default()
            }
            .to_json(),
        );
        let cursors = Arc::new(CursorStore::new());
        registry
            .ensure(&ch, &creds, None, Arc::clone(&cursors))
            .await
            .unwrap();
        assert_eq!(cursors.get("ch_wx_1").await, None);
        registry.stop_all().await;
    }

    /// 内存 InboundStateWriter：写回链路可注入
    struct MemWriter(tokio::sync::Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl InboundStateWriter for MemWriter {
        async fn save(&self, channel_id: &str, state: &InboundState) {
            self.0
                .lock()
                .await
                .push(format!("{}={}", channel_id, state.to_json()));
        }
    }

    #[tokio::test]
    async fn test_inbound_state_writer_trait_object() {
        let writer: Arc<dyn InboundStateWriter> =
            Arc::new(MemWriter(tokio::sync::Mutex::new(Vec::new())));
        writer.save("ch_1", &InboundState::default()).await;
    }
}
