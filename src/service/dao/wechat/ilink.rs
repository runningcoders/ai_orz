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
        Err(e) if e.is_timeout() => return Ok(IlinkUpdates::default()),
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
    Ok(IlinkUpdates { cursor, messages })
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

// ==================== 受管长轮询循环 ====================

/// 轮询循环句柄：任务 + 启动时凭证指纹（ensure 幂等 / 凭证变化自动重建）
struct PollLoopHandle {
    join: tokio::task::JoinHandle<()>,
    fingerprint: u64,
}

/// 连续失败重试节奏：前 5 次间隔 2s，超过后退避 30s（避免触发限流）
const FAIL_RETRY_FAST_MS: u64 = 2_000;
const FAIL_RETRY_SLOW_MS: u64 = 30_000;
const FAIL_FAST_LIMIT: u32 = 5;
/// 正常轮询间隙（长轮询本身 hold 35s，小幅间隔防紧密打转）
const POLL_PAUSE_MS: u64 = 500;

/// 长轮询循环体：收帧 publish 事件 + 刷会话 + 推进游标 + 一次写回
///
/// 终止方式：registry 移除句柄时 `abort()`。单 writer 独占 `inbound_state`，
/// abort 只可能损失"最后一轮"的状态写回，游标回退由事件幂等键兜底。
async fn poll_loop(
    channel_id: String,
    credentials: IlinkChannelCredentials,
    mut state: InboundState,
    writer: Option<Arc<dyn InboundStateWriter>>,
) {
    log_info!(
        "ilink poll loop started: channel_id={} bot_id={} base_url={}",
        channel_id,
        credentials.bot_id,
        credentials.base_url
    );
    let mut consecutive_failures: u32 = 0;
    loop {
        let cursor = state
            .cursor
            .as_ref()
            .map(|c| c.value.clone())
            .filter(|v| !v.is_empty());
        match get_updates(&credentials, cursor.as_deref()).await {
            Err(e) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let delay = if consecutive_failures <= FAIL_FAST_LIMIT {
                    FAIL_RETRY_FAST_MS
                } else {
                    FAIL_RETRY_SLOW_MS
                };
                log_warn!(
                    "ilink getupdates failed (retry in {}ms): channel_id={} failures={} err={}",
                    delay,
                    channel_id,
                    consecutive_failures,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }
            Ok(updates) => {
                consecutive_failures = 0;
                let IlinkUpdates {
                    cursor: new_cursor,
                    messages,
                } = updates;
                let message_count = messages.len();
                let now_ms = common::constants::utils::current_timestamp_ms();

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
                        message,
                    };
                    crate::pkg::aop::registry()
                        .publish(&RequestContext::new_system(), event)
                        .await;
                }

                // 推进游标：服务端返回新值才覆盖（Opaque：只能原样回传）
                let has_new_cursor = new_cursor.is_some();
                if let Some(cursor_value) = new_cursor {
                    state.cursor = Some(common::models::inbound_state::InboundCursor::opaque(
                        cursor_value,
                        "ilink",
                    ));
                    if let Some(c) = state.cursor.as_mut() {
                        c.updated_at_ms = Some(now_ms);
                    }
                }

                // 一次写回：游标 + 会话合并；有实际变化才落库（空轮询零写入）
                if (has_new_cursor || message_count > 0)
                    && let Some(writer) = &writer
                {
                    writer.save(&channel_id, &state).await;
                }
                tokio::time::sleep(std::time::Duration::from_millis(POLL_PAUSE_MS)).await;
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

        let channel_id = channel.id().to_string();
        let credentials = credentials.clone();
        let join = tokio::spawn(poll_loop(channel_id.clone(), credentials, state, writer));
        self.loops
            .write()
            .await
            .insert(channel_id.clone(), PollLoopHandle { join, fingerprint });
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

        registry.ensure(&ch, &creds, None).await.unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 同指纹：幂等（不重建）
        registry.ensure(&ch, &creds, None).await.unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 指纹变化：重建（stop + start）
        let mut creds2 = creds.clone();
        creds2.bot_token = "tok2".into();
        registry.ensure(&ch, &creds2, None).await.unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // stop 幂等
        assert!(registry.stop("ch_wx_1").await);
        assert!(!registry.is_running("ch_wx_1").await);
        assert!(!registry.stop("ch_wx_1").await);

        registry.stop_all().await;
        assert!(!registry.is_running("ch_wx_1").await);
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
