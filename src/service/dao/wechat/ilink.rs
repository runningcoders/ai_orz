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

use serde_json::Value;
use tokio::sync::RwLock;

use common::error::{Result, err};
use common::models::inbound_state::InboundState;

use crate::models::events::{IlinkMessage, WechatInboundEvent};
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use crate::pkg::wechat_ilink;

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

/// 长轮询**默认**客户端超时：服务端 hold ~35s，客户端必须大于它
///
/// 实际每次请求的超时由服务端建议值动态决定（见 [`CLIENT_TIMEOUT_MARGIN_MS`] 与
/// [`IlinkUpdates::longpolling_timeout_ms`]），本常量仅作首轮与缺省值。
pub(crate) const UPDATES_POLL_TIMEOUT_MS: u64 = 45_000;

/// 客户端超时相对服务端建议值的余量（方案 D3）
///
/// 官方直接采纳 `longpolling_timeout_ms`（客户端超时属正常控制流）；我方留 10s 余量，
/// 换取「客户端超时仍是异常信号」的监控语义（正常轮询不应触发客户端超时）。
const CLIENT_TIMEOUT_MARGIN_MS: u64 = 10_000;

/// 动态超时上限：防服务端给出异常大值 → 请求永不返回、监听形同停摆
const UPDATES_POLL_TIMEOUT_MAX_MS: u64 = 120_000;

/// 服务端「会话/令牌失效」错误码（官方 `STALE_TOKEN_ERRCODE`）
///
/// 官方对它的处置是**暂停该账号全部请求 1 小时**（`session-guard`）：此时继续轮询
/// 只会持续失败，须等用户重新扫码授权。此前我方完全不校验错误码、一律当空轮次 ——
/// 会话过期后**永远自愈不了**，且日志与「没人发消息」完全同形。
pub(crate) const STALE_TOKEN_ERRCODE: i64 = -14;

/// `notifystart` / `notifystop` 客户端超时（官方 `DEFAULT_CONFIG_TIMEOUT_MS`）
const NOTIFY_TIMEOUT_MS: u64 = 10_000;

/// getupdates 长轮询响应（游标 + 消息列表 + 错误码 + 超时协商）
#[derive(Debug, Clone, Default)]
pub struct IlinkUpdates {
    /// 新游标（`get_updates_buf`；服务端未返回时为 None，保持旧游标）
    pub cursor: Option<String>,
    /// 本轮消息列表
    pub messages: Vec<IlinkMessage>,
    /// 本轮是否因**客户端超时**返回（非服务端 hold 到期）
    ///
    /// 客户端超时 > 服务端 hold，正常轮询**永不**触发 —— 一旦为真即
    /// 网络 hang 或服务端异常。此前该分支直接返回空批次，导致「网络断了」与
    /// 「队列就是空的」在日志上完全同形、无法区分。
    pub client_timeout: bool,
    /// 服务端错误码（`ret` / `errcode`；0 = 正常）
    pub errcode: i64,
    /// 错误描述（`errmsg`，仅用于日志与可读报错）
    pub errmsg: String,
    /// 服务端建议的下一次长轮询客户端超时（`longpolling_timeout_ms`）
    pub longpolling_timeout_ms: Option<u64>,
}

impl IlinkUpdates {
    /// 服务端是否返回错误（此前该字段被完全忽略 → 会话过期表现为静默空轮次）
    pub fn is_error(&self) -> bool {
        self.errcode != 0
    }

    /// 是否为「会话/令牌失效」错误（须暂停而非继续轮询）
    pub fn is_stale_token(&self) -> bool {
        self.errcode == STALE_TOKEN_ERRCODE
    }
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

fn http_err(op: &str, e: reqwest::Error) -> common::error::Error {
    err!(ThirdPartyError, "ilink {} http error: {}", op, e)
}

/// 原始报文进日志的最大长度（超出截断；避免整包灌进日志）
const RAW_BODY_LOG_LIMIT: usize = 2_000;

/// 报文里需要脱敏的令牌字段（iLink 响应体会带滚动刷新的会话令牌）
const REDACT_KEYS: &[&str] = &["context_token", "bot_token", "typing_ticket"];

/// 报文脱敏：把令牌字段的字符串值替换为 `***`
///
/// 留痕路径不做完整 JSON 解析（避免再引入失败点），按 `"key":"value"` 的简单形态
/// 扫描；iLink 的令牌均为 base64/hex，不含转义引号，简单扫描足够。
fn redact_tokens(body: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        let pattern = format!(r#""({})":"[^"]*""#, REDACT_KEYS.join("|"));
        regex::Regex::new(&pattern).expect("iLink 脱敏正则编译失败")
    });
    re.replace_all(body, r#""$1":"***""#).into_owned()
}

/// 日志用截断（按字符切，避免切坏多字节），超出时附原始长度
fn truncate_for_log(value: &str, limit: usize) -> String {
    let total = value.chars().count();
    if total <= limit {
        return value.to_string();
    }
    let head: String = value.chars().take(limit).collect();
    format!("{head}…(truncated, {total} chars total)")
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
///
/// `timeout_ms` 由调用方按服务端建议值动态给出（首轮用 [`UPDATES_POLL_TIMEOUT_MS`]）；
/// 请求级超时覆盖客户端默认超时，因此不必为每个建议值重建客户端。
pub async fn get_updates(
    credentials: &IlinkChannelCredentials,
    cursor: Option<&str>,
    timeout_ms: u64,
) -> Result<IlinkUpdates> {
    let url = format!("{}/ilink/bot/getupdates", credentials.base_url);
    // `base_info`：官方 `getUpdates` 的请求体即 `{ get_updates_buf, base_info }`
    let body = serde_json::json!({
        "get_updates_buf": cursor.unwrap_or_default(),
        "base_info": wechat_ilink::base_info(),
    });
    let resp = wechat_ilink::bot_auth_headers(poll_client().post(&url), &credentials.bot_token)
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        // 客户端超时：标记后交回循环计数（见 `IlinkUpdates::client_timeout`）。
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
/// 单条消息解析失败**留痕后跳过**（不因脏数据中断整批，但绝不静默——`.ok()` 静默吞错
/// 会让「字段对不上」与「上游没数据」在日志上完全同形，是上次误判的根因）。
fn parse_updates(body: &str) -> Result<IlinkUpdates> {
    // 原始响应体留痕（一次性实证，方案 A6）：线上 `message_type` 究竟是数字还是
    // 字符串、游标字段名等，只有真实帧能给出定论。debug 级 + 截断 + 令牌脱敏，
    // 首轮真机确认线上形态后可摘除或降噪。
    log_debug!(
        "ilink getupdates raw response ({} bytes): {}",
        body.len(),
        truncate_for_log(&redact_tokens(body), RAW_BODY_LOG_LIMIT)
    );

    let value: Value = serde_json::from_str(body)
        .map_err(|e| err!(ThirdPartyError, "ilink getupdates 响应非 JSON: {}", e))?;

    let cursor = value
        .get("get_updates_buf")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from);
    // 错误码：官方 `GetUpdatesResp` 的 `ret` 与 `errcode` 双字段，任一非 0 即错误
    let errcode = value
        .get("ret")
        .or_else(|| value.get("errcode"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let errmsg = value
        .get("errmsg")
        .or_else(|| value.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // 服务端建议的下一次长轮询超时（官方采纳它调整客户端超时）
    let longpolling_timeout_ms = value
        .get("longpolling_timeout_ms")
        .and_then(Value::as_u64)
        .filter(|ms| *ms > 0);

    let raw_messages = value
        .get("msg_list")
        .or_else(|| value.get("msgs"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut messages = Vec::with_capacity(raw_messages.len());
    for (index, raw) in raw_messages.into_iter().enumerate() {
        match serde_json::from_value::<IlinkMessage>(raw.clone()) {
            Ok(message) => messages.push(message),
            // 留痕而非静默：单条脏数据不中断整批，但必须能在日志里看见
            Err(e) => log_warn!(
                "ilink message parse failed (skipped): index={} err={} raw={}",
                index,
                e,
                truncate_for_log(&redact_tokens(&raw.to_string()), RAW_BODY_LOG_LIMIT)
            ),
        }
    }

    Ok(IlinkUpdates {
        cursor,
        messages,
        errcode,
        errmsg,
        longpolling_timeout_ms,
        // 能解析出响应体 = 服务端已返回，非客户端超时
        client_timeout: false,
    })
}

/// 构造 sendmessage 请求体（抽纯函数便于单测）
///
/// 严格按官方 `send.ts` 的 `SendMessageReq` 形态：`from_user_id` 留空、
/// `to_user_id` 填对端、`context_token` 回传收到的最新值、文本走 `item_list` 的
/// **`text_item.text`**，枚举一律**数字**（此前写成 `"BOT"` / `"FINISH"` 与 `content`，
/// 三处都与官方不符）。另注意官方 `sendmessage` 的请求体**只有 `msg`、不含 `base_info`**，
/// 此处对齐官方，不画蛇添足。
fn build_send_body(to_user_id: &str, context_token: &str, text: &str, client_id: &str) -> Value {
    serde_json::json!({
        "msg": {
            "from_user_id": "",
            "to_user_id": to_user_id,
            "client_id": client_id,
            "message_type": wechat_ilink::MESSAGE_TYPE_BOT,
            "message_state": wechat_ilink::MESSAGE_STATE_FINISH,
            "item_list": [
                {
                    "type": wechat_ilink::MESSAGE_ITEM_TYPE_TEXT,
                    "text_item": { "text": text }
                }
            ],
            "context_token": context_token,
        }
    })
}

/// 发送文本消息到对端（出站）
///
/// 返回服务端 `SendMessageResp.message_id`（平台侧权威消息 ID，用于回写
/// `messages.external_key`，与飞书 `lark:{message_id}` 口径对齐）；服务端未返回该字段时
/// 为 `None`（不伪造）。HTTP 2xx 且 `ret == 0` 即视为发送成功。
pub async fn send_text(
    credentials: &IlinkChannelCredentials,
    to_user_id: &str,
    context_token: &str,
    text: &str,
) -> Result<Option<String>> {
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
    let resp = wechat_ilink::bot_auth_headers(client().post(&url), &credentials.bot_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| http_err("sendmessage", e))?;
    let resp = resp
        .error_for_status()
        .map_err(|e| http_err("sendmessage", e))?;
    let body_text = resp.text().await.map_err(|e| http_err("sendmessage", e))?;
    parse_send_response(&body_text)
}

/// 解析 sendmessage 响应：校验 `ret` 并取出服务端 `message_id`（抽纯函数便于单测）
///
/// 返回值为服务端权威消息 ID（`message_id`，uint64 按数字/字符串双形态读），
/// 缺省时 `None` —— 不伪造，由调用方决定是否回写 `external_key`。
fn parse_send_response(body: &str) -> Result<Option<String>> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        // 响应非 JSON：HTTP 2xx 已通过，宽容视为成功（协议较新）
        return Ok(None);
    };
    let ret = value
        .get("ret")
        .or_else(|| value.get("errcode"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
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
    Ok(value.get("message_id").and_then(|v| match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }))
}

// ==================== 会话启停通知 ====================

/// 通知服务端「本客户端已启动」（官方 `ilink/bot/msg/notifystart`）
///
/// 官方在渠道启动时调用一次，失败仅告警——服务端据此知道客户端在册。
pub async fn notify_start(credentials: &IlinkChannelCredentials) -> Result<()> {
    notify(credentials, "notifystart").await
}

/// 通知服务端「本客户端将停止」（官方 `ilink/bot/msg/notifystop`）
///
/// 官方在渠道停止/网关退出时调用一次（独立超时，不随长轮询一起被 abort），失败仅告警。
pub async fn notify_stop(credentials: &IlinkChannelCredentials) -> Result<()> {
    notify(credentials, "notifystop").await
}

/// 启停通知的公共实现（`{ base_info }` 请求体 + 10s 超时）
async fn notify(credentials: &IlinkChannelCredentials, endpoint: &str) -> Result<()> {
    let url = format!("{}/ilink/bot/msg/{endpoint}", credentials.base_url);
    let body = serde_json::json!({ "base_info": wechat_ilink::base_info() });
    let resp = wechat_ilink::bot_auth_headers(client().post(&url), &credentials.bot_token)
        .timeout(std::time::Duration::from_millis(NOTIFY_TIMEOUT_MS))
        .json(&body)
        .send()
        .await
        .map_err(|e| http_err(endpoint, e))?
        .error_for_status()
        .map_err(|e| http_err(endpoint, e))?;
    let text = resp.text().await.map_err(|e| http_err(endpoint, e))?;
    // 响应错误码只作提示（官方对 `notifyStart` 的非 0 `ret` 亦仅 warn）
    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let ret = value
        .get("ret")
        .or_else(|| value.get("errcode"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if ret != 0 {
        // ⚠️ 先取出局部变量再交给日志宏：`tracing` 的宏展开会把裸 `Value` 路径解析成
        // `tracing::field::Value`（trait）而非 `serde_json::Value`（struct），
        // 在宏参数里写 `Value::as_str` 会报 E0782「expected a type, found a trait」。
        let errmsg = value.get("errmsg").and_then(Value::as_str).unwrap_or("");
        log_warn!("ilink {} returned ret={} errmsg={}", endpoint, ret, errmsg);
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

// ==================== 会话暂停（-14 自愈）====================

/// 会话暂停时长（官方 `session-guard` 的 `SESSION_PAUSE_DURATION_MS = 1h`）
const SESSION_PAUSE_MS: u64 = 3_600_000;

/// 暂停期内的休眠分片：不一次睡满 1 小时，便于 `registry.stop` 的 abort 及时生效、
/// 且到期后能立刻恢复轮询
const PAUSE_SLEEP_CHUNK_MS: i64 = 60_000;

/// 会话暂停表（进程内，channel_id 键控）
///
/// 服务端返回 [`STALE_TOKEN_ERRCODE`] 时暂停该 channel 的**全部**请求——继续轮询只会
/// 持续失败，须等用户重新扫码授权。官方 `session-guard` 语义相同（1 小时 + 出站一并拦）。
///
/// 落点选择（方案 D4）为**进程内**：暂停是自愈手段，进程重启后重试即恢复；
/// 落库会多出一处「必须两端闭合」的持久化运行态（参见游标回灌那一课），收益不匹配。
#[derive(Default)]
pub(crate) struct SessionGuard {
    paused_until_ms: RwLock<HashMap<String, i64>>,
}

impl SessionGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// 暂停该 channel 的请求，返回解禁时间戳（ms）
    pub async fn pause(&self, channel_id: &str) -> i64 {
        let until = common::constants::utils::current_timestamp_ms() + SESSION_PAUSE_MS as i64;
        self.paused_until_ms
            .write()
            .await
            .insert(channel_id.to_string(), until);
        until
    }

    /// 剩余暂停毫秒（0 = 未暂停；已到期顺手清理，避免表无限增长）
    pub async fn remaining_ms(&self, channel_id: &str) -> i64 {
        let mut map = self.paused_until_ms.write().await;
        let Some(until) = map.get(channel_id).copied() else {
            return 0;
        };
        let remaining = until - common::constants::utils::current_timestamp_ms();
        if remaining <= 0 {
            map.remove(channel_id);
            return 0;
        }
        remaining
    }

    /// 是否处于暂停中
    pub async fn is_paused(&self, channel_id: &str) -> bool {
        self.remaining_ms(channel_id).await > 0
    }

    /// 解除暂停（凭证轮换 / 重新扫码后由调用方显式清除）
    pub async fn clear(&self, channel_id: &str) {
        self.paused_until_ms.write().await.remove(channel_id);
    }
}

// ==================== 受管长轮询循环 ====================

/// 轮询循环句柄：任务 + 启动时凭证指纹（ensure 幂等 / 凭证变化自动重建）
struct PollLoopHandle {
    join: tokio::task::JoinHandle<()>,
    fingerprint: u64,
    /// 运行态快照（监控读取入口，与飞书 `WsClientState.conn_state` 同构）
    stats: Arc<RwLock<PollRuntimeStats>>,
    /// 停止通知（`notifystop`）所需凭证：循环被 `abort()` 后无法在循环内发出，
    /// 只能由 `stop` / `stop_all` 在句柄上补发
    credentials: IlinkChannelCredentials,
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
    /// 会话暂停解禁时间（ms；0 = 未暂停。由 `-14` 触发，见 [`SessionGuard`]）
    paused_until_ms: i64,
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
            paused_until_ms: 0,
        }
    }

    /// 轮询阶段：暂停中 → `paused`；连续失败 → `degraded`（退避中）；否则 `polling`
    ///
    /// `paused` 优先于 `degraded`：暂停是**外部原因**（须重新授权），不是网络抖动，
    /// 两者的处置完全不同，混在一个态里会让运维误判为「等一会就好」。
    fn state(&self) -> &'static str {
        if self.paused_until_ms > 0 {
            "paused"
        } else if self.consecutive_failures > 0 {
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
    guard: Arc<SessionGuard>,
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

    // 启停通知（方案 B4）：官方在渠道启动时调一次；失败仅告警、不阻塞轮询
    if let Err(e) = notify_start(&credentials).await {
        log_warn!(
            "ilink notifystart failed (ignored): channel_id={} err={}",
            channel_id,
            e
        );
    }

    // 心跳基准（仅日志用；其余计数一律以 `stats` 为唯一存储，避免双份漂移）
    let started_at_ms = common::constants::utils::current_timestamp_ms();
    let mut last_heartbeat_ms = started_at_ms;
    // 动态客户端超时（方案 D3）：首轮用默认值，之后按服务端 `longpolling_timeout_ms` 调整
    let mut next_timeout_ms = UPDATES_POLL_TIMEOUT_MS;
    // 暂停标记：避免暂停期内每轮重复写运行态快照
    let mut paused_marked = false;

    loop {
        // 0. 暂停期（`-14` 自愈）：不发请求，休眠到解禁
        let remaining = guard.remaining_ms(&channel_id).await;
        if remaining > 0 {
            if !paused_marked {
                let mut s = stats.write().await;
                s.paused_until_ms = common::constants::utils::current_timestamp_ms() + remaining;
                paused_marked = true;
            }
            log_warn!(
                "ilink session paused, polling suspended: channel_id={} remaining_ms={} (rescan to re-authorize)",
                channel_id,
                remaining
            );
            tokio::time::sleep(std::time::Duration::from_millis(
                remaining.min(PAUSE_SLEEP_CHUNK_MS) as u64,
            ))
            .await;
            continue;
        }
        if paused_marked {
            paused_marked = false;
            let mut s = stats.write().await;
            s.paused_until_ms = 0;
            drop(s);
            log_info!(
                "ilink session pause elapsed, resuming polling: channel_id={}",
                channel_id
            );
        }

        // 请求游标 = **已确认**消费的游标（P2）：上一轮没消费完则不动，下一轮重拉
        let cursor = cursors.get(&channel_id).await;
        match get_updates(&credentials, cursor.as_deref(), next_timeout_ms).await {
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
                // 1. 服务端错误码（方案 B1/B2）：此前**完全不校验**，一律当空轮次 ——
                //    会话过期后永不恢复，且日志与「没人发消息」完全同形。
                if updates.is_error() {
                    if updates.is_stale_token() {
                        // `-14`：暂停该 channel 全部请求（官方 session-guard 口径）
                        let until = guard.pause(&channel_id).await;
                        {
                            let mut s = stats.write().await;
                            s.paused_until_ms = until;
                            s.consecutive_failures = 0;
                        }
                        paused_marked = true;
                        log_error!(
                            "ilink getupdates stale token (errcode={}), pausing channel for {}min: channel_id={} errmsg={} until_ms={} — 需重新扫码授权",
                            updates.errcode,
                            SESSION_PAUSE_MS / 60_000,
                            channel_id,
                            updates.errmsg,
                            until
                        );
                        continue;
                    }
                    // 其他错误码：计入连续失败走退避（官方仅对 `-14` 有特殊处置）
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
                        "ilink getupdates api error (retry in {}ms): channel_id={} errcode={} errmsg={} failures={}",
                        delay,
                        channel_id,
                        updates.errcode,
                        updates.errmsg,
                        failures
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    continue;
                }

                // 2. 超时协商（方案 D3）：采纳服务端建议值 + 余量，并夹在上限内
                if let Some(server_ms) = updates.longpolling_timeout_ms {
                    let adjusted = server_ms
                        .saturating_add(CLIENT_TIMEOUT_MARGIN_MS)
                        .min(UPDATES_POLL_TIMEOUT_MAX_MS);
                    if adjusted != next_timeout_ms {
                        log_info!(
                            "ilink poll timeout adjusted by server suggestion: channel_id={} server_ms={} next_timeout_ms={}",
                            channel_id,
                            server_ms,
                            adjusted
                        );
                        next_timeout_ms = adjusted;
                    }
                }

                let IlinkUpdates {
                    cursor: new_cursor,
                    messages,
                    client_timeout,
                    ..
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

                // 客户端超时（正常永不触发）：与「服务端 hold 到期返回空批次」分开计数。
                // 官方视其为**正常控制流**（其客户端超时与服务端 hold 相等），我方留了
                // 10s 余量，故这里降为 debug —— 异常仍由计数与 `last_poll_at_ms` 停摆暴露。
                if client_timeout {
                    log_debug!(
                        "ilink getupdates client timeout ({}ms; server holds ~35s): channel_id={} timeouts={}",
                        next_timeout_ms,
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
                        "ilink poll heartbeat: channel_id={} state={} rounds={} inbound_messages={} consecutive_failures={} client_timeouts={} cursor={} idle_ms={}",
                        channel_id,
                        s.state(),
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
        guard: Arc<SessionGuard>,
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

        // 走到这里意味着"未运行或凭证已变"，典型来源就是用户重新扫码授权：
        // 清除该 channel 的 `-14` 会话暂停，让新 token 立刻获得机会。
        // 不清则重扫成功后仍要干等到暂停到期——用户看到"授权成功了但收不到消息"。
        if guard.is_paused(channel.id()).await {
            guard.clear(channel.id()).await;
            log_info!(
                "ilink session pause cleared (credentials changed): channel_id={}",
                channel.id()
            );
        }

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
            credentials.clone(),
            state,
            writer,
            cursors,
            guard,
            stats.clone(),
        ));
        self.loops.write().await.insert(
            channel_id.clone(),
            PollLoopHandle {
                join,
                fingerprint,
                stats,
                credentials,
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
    ///
    /// 顺带通知服务端「本客户端将停止」（方案 B4）。循环任务已 `abort()`，通知只能在
    /// 句柄上补发；失败仅告警——通知用于服务端观测/协调，不影响本地状态。
    pub async fn stop(&self, channel_id: &str) -> bool {
        let handle = self.loops.write().await.remove(channel_id);
        match handle {
            Some(handle) => {
                handle.join.abort();
                if let Err(e) = notify_stop(&handle.credentials).await {
                    log_warn!(
                        "ilink notifystop failed (ignored): channel_id={} err={}",
                        channel_id,
                        e
                    );
                }
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
        for (channel_id, handle) in handles {
            handle.join.abort();
            if let Err(e) = notify_stop(&handle.credentials).await {
                log_warn!(
                    "ilink notifystop failed (ignored): channel_id={} err={}",
                    channel_id,
                    e
                );
            }
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
                paused_until_ms: s.paused_until_ms,
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

    /// getupdates 解析：游标 + 消息列表 + 超时协商；脏消息跳过但不中断整批
    #[test]
    fn test_parse_updates() {
        // 官方形态：数字枚举 + `text` 字段
        let body = r#"{
            "ret": 0,
            "get_updates_buf": "cur_abc",
            "longpolling_timeout_ms": 35000,
            "msgs": [
                {"from_user_id":"p1","message_id":"9001","client_id":"c1","message_type":1,"message_state":2,"context_token":"t1","item_list":[{"type":1,"text_item":{"text":"hi"}}]},
                {"item_list": 1}
            ]
        }"#;
        let updates = parse_updates(body).unwrap();
        assert_eq!(updates.cursor.as_deref(), Some("cur_abc"));
        assert_eq!(updates.messages.len(), 1, "脏消息应被跳过而非中断整批");
        assert_eq!(updates.messages[0].from_user_id, "p1");
        assert_eq!(updates.messages[0].text(), Some("hi".to_string()));
        assert!(!updates.is_error());
        assert_eq!(updates.longpolling_timeout_ms, Some(35_000));

        // 早期形态兼容：`msg_list` 字段名 + 字符串枚举 + `content` 文本字段
        let legacy = r#"{"msg_list":[{"from_user_id":"p2","client_id":"c2","message_type":"USER","message_state":"FINISH","item_list":[{"type":1,"text_item":{"content":"旧"}}]}]}"#;
        let updates = parse_updates(legacy).unwrap();
        assert_eq!(updates.cursor, None);
        assert_eq!(updates.messages.len(), 1);
        assert_eq!(updates.messages[0].message_key(), "c2");
        assert_eq!(updates.messages[0].text(), Some("旧".to_string()));

        // 空响应（服务端 hold 到期）
        let updates = parse_updates(r#"{"ret":0}"#).unwrap();
        assert!(updates.messages.is_empty());
        assert_eq!(updates.cursor, None);
        assert_eq!(updates.longpolling_timeout_ms, None);
    }

    /// 错误码：`ret` / `errcode` 任一非 0 即错误；`-14` 单独识别为会话失效
    ///
    /// 回归护栏：此前这两个字段**被完全忽略**，会话过期后表现为「静默空轮次」，
    /// 与「没人发消息」在日志上完全同形。
    #[test]
    fn test_parse_updates_error_codes() {
        let updates = parse_updates(r#"{"ret":-14,"errmsg":"session expired"}"#).unwrap();
        assert!(updates.is_error());
        assert!(updates.is_stale_token());
        assert_eq!(updates.errmsg, "session expired");

        let updates = parse_updates(r#"{"errcode":1001,"errmsg":"boom"}"#).unwrap();
        assert!(updates.is_error());
        assert!(!updates.is_stale_token(), "只有 -14 触达暂停");

        // 0 / 缺失都算正常
        assert!(!parse_updates(r#"{"ret":0}"#).unwrap().is_error());
        assert!(!parse_updates(r#"{}"#).unwrap().is_error());
    }

    /// 报文脱敏：令牌字段值被替换，其余内容保留（留痕路径不得泄漏会话令牌）
    #[test]
    fn test_redact_tokens() {
        let body =
            r#"{"msgs":[{"context_token":"tok_secret_value","item_list":[]}],"bot_token":"bt"}"#;
        let redacted = redact_tokens(body);
        assert!(!redacted.contains("tok_secret_value"));
        assert!(!redacted.contains("\"bt\""));
        assert!(redacted.contains(r#""context_token":"***""#));
        assert!(redacted.contains(r#""bot_token":"***""#));
        // 无令牌的报文原样返回
        let plain = r#"{"ret":0,"msgs":[]}"#;
        assert_eq!(redact_tokens(plain), plain);
    }

    /// 截断：短串原样、长串截断并附原始长度（按字符切，不切坏多字节）
    #[test]
    fn test_truncate_for_log() {
        assert_eq!(truncate_for_log("abc", 8), "abc");
        assert_eq!(truncate_for_log("12345678", 8), "12345678");
        assert_eq!(
            truncate_for_log("123456789", 8),
            "12345678…(truncated, 9 chars total)"
        );
        assert!(truncate_for_log("中文测试超长内容", 4).starts_with("中文测试"));
    }

    /// sendmessage 请求体：from 留空 / to 填对端 / context_token 回传 /
    /// 枚举为**数字** / 文本走 `text_item.text` / 不带 `base_info`
    ///
    /// 这条同时是回归护栏：此前 `"BOT"` / `"FINISH"` / `content` 三处都与官方不符。
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
        // 官方是数字枚举（`2 = BOT` / `2 = FINISH`）
        assert_eq!(msg.get("message_type").and_then(Value::as_i64), Some(2));
        assert_eq!(msg.get("message_state").and_then(Value::as_i64), Some(2));
        let items = msg.get("item_list").and_then(Value::as_array).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get("type").and_then(Value::as_i64), Some(1));
        assert_eq!(
            items[0]
                .get("text_item")
                .unwrap()
                .get("text")
                .and_then(Value::as_str),
            Some("回复内容")
        );
        // 官方 `sendmessage` 请求体只有 `msg`，不带 `base_info`
        assert!(body.get("base_info").is_none());
    }

    /// 出站请求头对齐官方 `buildHeaders`：身份两件套 + 鉴权 + UIN（十进制字符串 base64）
    #[test]
    fn test_bot_auth_headers_are_aligned() {
        let builder = wechat_ilink::bot_auth_headers(
            crate::pkg::http::presets::outbound()
                .build()
                .unwrap()
                .post("https://example.com"),
            "tok_x",
        );
        let request = builder.body("{}").build().unwrap();
        let headers = request.headers();
        assert_eq!(
            headers.get("iLink-App-Id").and_then(|v| v.to_str().ok()),
            Some("bot")
        );
        assert_eq!(
            headers
                .get("iLink-App-ClientVersion")
                .and_then(|v| v.to_str().ok()),
            Some(wechat_ilink::ILINK_APP_CLIENT_VERSION.to_string().as_str())
        );
        assert_eq!(
            headers
                .get("AuthorizationType")
                .and_then(|v| v.to_str().ok()),
            Some("ilink_bot_token")
        );
        assert_eq!(
            headers.get("Authorization").and_then(|v| v.to_str().ok()),
            Some("Bearer tok_x")
        );
        // X-WECHAT-UIN = base64(十进制字符串)，解出来应是纯数字
        let uin = headers
            .get("X-WECHAT-UIN")
            .and_then(|v| v.to_str().ok())
            .expect("X-WECHAT-UIN 必须存在");
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(uin)
            .expect("UIN 应为合法 base64");
        let decoded = String::from_utf8(decoded).expect("UIN 解出应为 UTF-8");
        assert!(
            !decoded.is_empty() && decoded.chars().all(|c| c.is_ascii_digit()),
            "UIN 解码后应为十进制字符串，实际 {decoded:?}"
        );
    }

    /// 版本编码：官方 `0x00MMNNPP` 规则（`1.0.11` → 65547）
    #[test]
    fn test_encode_client_version() {
        assert_eq!(wechat_ilink::encode_client_version("1.0.11"), 0x0001_000B);
        assert_eq!(wechat_ilink::encode_client_version("2.4.9"), 132_105);
        assert_eq!(wechat_ilink::encode_client_version("1.2"), 0x0001_0200);
        assert_eq!(wechat_ilink::encode_client_version("1"), 0x0001_0000);
        // 畸形输入不 panic（缺失段按 0）
        assert_eq!(wechat_ilink::encode_client_version(""), 0);
        assert_eq!(wechat_ilink::encode_client_version("x.y.z"), 0);
    }

    /// sendmessage 响应解析：ret=0 通过并取 `message_id` / 非 JSON 宽容 / ret!=0 报错
    #[test]
    fn test_parse_send_response() {
        assert_eq!(parse_send_response(r#"{"ret":0}"#).unwrap(), None);
        assert_eq!(parse_send_response("ok").unwrap(), None);
        // 服务端权威 ID：字符串与数字两种形态都读
        assert_eq!(
            parse_send_response(r#"{"ret":0,"message_id":"m1"}"#).unwrap(),
            Some("m1".to_string())
        );
        assert_eq!(
            parse_send_response(r#"{"ret":0,"message_id":7400000000000000123}"#).unwrap(),
            Some("7400000000000000123".to_string())
        );
        let e = parse_send_response(r#"{"ret":1001,"errmsg":"token expired"}"#).unwrap_err();
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
            .ensure(
                &ch,
                &creds,
                None,
                Arc::new(CursorStore::new()),
                Arc::new(SessionGuard::new()),
            )
            .await
            .unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 同指纹：幂等（不重建）
        registry
            .ensure(
                &ch,
                &creds,
                None,
                Arc::new(CursorStore::new()),
                Arc::new(SessionGuard::new()),
            )
            .await
            .unwrap();
        assert!(registry.is_running("ch_wx_1").await);

        // 指纹变化：重建（stop + start）
        let mut creds2 = creds.clone();
        creds2.bot_token = "tok2".into();
        registry
            .ensure(
                &ch,
                &creds2,
                None,
                Arc::new(CursorStore::new()),
                Arc::new(SessionGuard::new()),
            )
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

    /// 轮询阶段判定：`paused` > `degraded` > `polling`
    ///
    /// 暂停优先于降级是刻意的：两者处置完全不同（须重新授权 vs 等一会自愈），
    /// 混在一个态里会让运维误判为「等一会就好」。
    #[test]
    fn test_poll_runtime_stats_state() {
        let mut s = PollRuntimeStats::new("我的微信".to_string(), "bot_1".to_string());
        assert_eq!(s.state(), "polling");
        s.consecutive_failures = 1;
        assert_eq!(s.state(), "degraded");
        s.paused_until_ms = 1;
        assert_eq!(s.state(), "paused");
        s.paused_until_ms = 0;
        s.consecutive_failures = 0;
        assert_eq!(s.state(), "polling");
    }

    /// 会话暂停表：pause 后进入暂停、clear 立即解除、channel 隔离
    #[tokio::test]
    async fn test_session_guard_lifecycle() {
        let guard = SessionGuard::new();
        assert!(!guard.is_paused("ch_a").await);
        assert_eq!(guard.remaining_ms("ch_a").await, 0);

        let until = guard.pause("ch_a").await;
        assert!(until > common::constants::utils::current_timestamp_ms());
        assert!(guard.is_paused("ch_a").await);
        assert!(guard.remaining_ms("ch_a").await > 0);
        // channel 隔离：暂停一个渠道不影响另一个
        assert!(!guard.is_paused("ch_b").await);

        guard.clear("ch_a").await;
        assert!(!guard.is_paused("ch_a").await);
    }

    /// 暂停到期自动清理（否则表会随渠道数无限增长）
    #[tokio::test]
    async fn test_session_guard_expired_is_cleared() {
        let guard = SessionGuard::new();
        guard.paused_until_ms.write().await.insert(
            "ch_a".to_string(),
            common::constants::utils::current_timestamp_ms() - 1,
        );
        assert_eq!(guard.remaining_ms("ch_a").await, 0);
        assert!(guard.paused_until_ms.read().await.is_empty());
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
            .ensure(
                &ch,
                &creds,
                None,
                cursors.clone(),
                Arc::new(SessionGuard::new()),
            )
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
            .ensure(
                &ch,
                &creds,
                None,
                cursors.clone(),
                Arc::new(SessionGuard::new()),
            )
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
            .ensure(
                &ch,
                &creds,
                None,
                Arc::clone(&cursors),
                Arc::new(SessionGuard::new()),
            )
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
            .ensure(
                &channel(),
                &creds,
                None,
                Arc::clone(&cursors),
                Arc::new(SessionGuard::new()),
            )
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
            .ensure(
                &ch,
                &creds,
                None,
                Arc::clone(&cursors),
                Arc::new(SessionGuard::new()),
            )
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
