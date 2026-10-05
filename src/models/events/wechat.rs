//! 微信 iLink 事件类型定义（AOP 事件统一目录）
//!
//! 覆盖 iLink `getupdates` 返回的消息条目结构与 AOP 入站事件信封。
//! 事件类型为纯数据（serde DTO + Event impl），归属 models 层；
//! DAO 侧长轮询 adapter 收帧解析后 publish，消费在 `consumer/wechat_inbound`。
//!
//! 协议参考（权威 spec）：腾讯官方插件 `@tencent-weixin/openclaw-weixin` 的
//! `src/api/types.ts`，取值常量见 [`crate::pkg::wechat_ilink`]（协议 SSOT）。
//!
//! iLink 为 2026 年新协议，字段解析全部宽容（`serde(default)` + 未知字段忽略 +
//! 「数字主 / 字符串兼容」双形态），协议漂移只需调整本文件与 `dao/wechat/ilink.rs`。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::pkg::wechat_ilink::{
    MESSAGE_STATE_FINISH, MESSAGE_STATE_GENERATING, MESSAGE_STATE_NEW, MESSAGE_TYPE_BOT,
    MESSAGE_TYPE_NONE, MESSAGE_TYPE_USER,
};

// ==================== 协议枚举（数字主 + 字符串兼容）====================

/// 解析「数字主 + 字符串兼容」的协议枚举值
///
/// - 数字：原样取（容忍 `1.0` 这类被 JSON 解析成浮点的形态）
/// - 字符串：先按十进制解析（`"1"`），再按名字表匹配（大小写不敏感）
/// - 其他 / 未命中：`default` —— 协议较新，宽容解析而非让整条消息解析失败
fn parse_wire_enum(value: &Value, names: &[(&str, i64)], default: i64) -> i64 {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(default),
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return default;
            }
            if let Ok(v) = trimmed.parse::<i64>() {
                return v;
            }
            names
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(trimmed))
                .map(|(_, v)| *v)
                .unwrap_or(default)
        }
        _ => default,
    }
}

/// iLink 消息方向（官方 `MessageType`：`0=NONE` / `1=USER` / `2=BOT`）
///
/// ⚠️ 官方线上形态是**数字**，此前声明为 `String` 并比较 `"USER"` —— serde 遇数字
/// 反序列化 `String` 会**整条失败**，而失败被解析层的 `.ok()` 吞掉，表现为
/// 「一帧都没收到」（连日志都不打）。现按本仓宽容解析约定做成双形态都收，
/// 谓词一律按数字判定，出站恒定输出数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IlinkMessageType(pub i64);

impl IlinkMessageType {
    /// 未设置
    pub const NONE: Self = Self(MESSAGE_TYPE_NONE);
    /// 对端用户发来
    pub const USER: Self = Self(MESSAGE_TYPE_USER);
    /// 本 bot 发出（入站回声需过滤）
    pub const BOT: Self = Self(MESSAGE_TYPE_BOT);

    /// 数字形态的值
    pub fn value(self) -> i64 {
        self.0
    }
}

impl Serialize for IlinkMessageType {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for IlinkMessageType {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(parse_wire_enum(
            &value,
            &[
                ("NONE", MESSAGE_TYPE_NONE),
                ("USER", MESSAGE_TYPE_USER),
                ("BOT", MESSAGE_TYPE_BOT),
            ],
            MESSAGE_TYPE_NONE,
        )))
    }
}

/// iLink 消息状态（官方 `MessageState`：`0=NEW` / `1=GENERATING` / `2=FINISH`）
///
/// 与 [`IlinkMessageType`] 同因同解：数字主 + 字符串兼容，出站恒定输出数字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IlinkMessageState(pub i64);

impl IlinkMessageState {
    /// 新消息
    pub const NEW: Self = Self(MESSAGE_STATE_NEW);
    /// 生成中（分片中间态）
    pub const GENERATING: Self = Self(MESSAGE_STATE_GENERATING);
    /// 完整（可投递）
    pub const FINISH: Self = Self(MESSAGE_STATE_FINISH);

    /// 数字形态的值
    pub fn value(self) -> i64 {
        self.0
    }
}

impl Serialize for IlinkMessageState {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for IlinkMessageState {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(parse_wire_enum(
            &value,
            &[
                ("NEW", MESSAGE_STATE_NEW),
                ("GENERATING", MESSAGE_STATE_GENERATING),
                ("FINISH", MESSAGE_STATE_FINISH),
            ],
            MESSAGE_STATE_NEW,
        )))
    }
}

// ==================== iLink 消息 DTO ====================

/// iLink 消息条目（`getupdates` 响应 `msgs` 的元素）
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkMessage {
    /// 发送者（对端微信用户标识，稳定，等同 openid 语义）
    #[serde(default)]
    pub from_user_id: String,
    /// 接收者（bot 侧标识）
    #[serde(default)]
    pub to_user_id: String,
    /// 服务端权威消息 ID（顶层 `message_id`，`uint64 on the wire`，按字符串无损）
    ///
    /// ⚠️ 字段层级纠正：官方顶层字段名是 `message_id`（`client_id` 由**对端客户端**
    /// 生成，`msg_id` 在 `MessageItem` 上）。此前把 `msg_id` 放在顶层属**字段错位**，
    /// 大概率永不命中，导致幂等键实际只有 `client_id` 在生效。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Value>,
    /// 客户端消息 ID（对端客户端生成；出站时作为本地幂等键）
    #[serde(default)]
    pub client_id: String,
    /// 消息方向（官方为数字）
    #[serde(default)]
    pub message_type: IlinkMessageType,
    /// 消息状态（官方为数字）
    #[serde(default)]
    pub message_state: IlinkMessageState,
    /// 会话上下文令牌（滚动刷新；回消息时必须回传最新值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// 消息条目（文本 / 图片 / 视频 / 文件 / 语音；阶段一只处理文本）
    #[serde(default)]
    pub item_list: Vec<IlinkMessageItem>,
}

/// iLink 消息内容条目
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkMessageItem {
    /// 条目类型（协议为数字，`1=文本`；保留原始值防协议漂移）
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<Value>,
    /// item 级消息 ID（官方 `MessageItem.msg_id`；顶层 `message_id` 缺失时作幂等兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<Value>,
    /// 文本条目（非文本消息为 None，由上层按需扩展）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_item: Option<IlinkTextItem>,
}

/// iLink 文本条目
///
/// ⚠️ 字段名纠正：官方是 `text`（`interface TextItem { text?: string }`），此前写成
/// `content` → [`IlinkMessage::text`] 恒返回 `None`，收发双向都断。此处做**双读**
/// （`text` 优先、`content` 兼容兜底），出站只发 `text`（以官方 spec 为准，不双发）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IlinkTextItem {
    /// 文本内容（官方字段名）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// 早期误用的字段名（仅入站兼容读；出站不写）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl IlinkTextItem {
    /// 文本内容：`text` 优先，`content` 兼容兜底
    pub fn value(&self) -> Option<&str> {
        self.text.as_deref().or(self.content.as_deref())
    }
}

/// `Value` → 幂等键字符串（数字走 `to_string`，与字符串形态归一）
fn value_to_key(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl IlinkMessage {
    /// 是否为对端用户发来的消息（过滤 BOT 自身回声）
    pub fn is_user(&self) -> bool {
        self.message_type == IlinkMessageType::USER
    }

    /// 是否为可投递的完整消息：仅排除「生成中」（`GENERATING`）
    ///
    /// 官方循环不对状态做过滤（它对 `msgs` 里每一条都处理）；我方保留该守卫仅为了
    /// 不投递流式分片的中间态。`NEW`(0) 与字段缺失一并放行 —— 协议较新，宁可宽容。
    pub fn is_finished(&self) -> bool {
        self.message_state != IlinkMessageState::GENERATING
    }

    /// 提取文本内容（首个非空文本条目；非文本消息返回 None）
    pub fn text(&self) -> Option<String> {
        self.item_list
            .iter()
            .filter_map(|item| item.text_item.as_ref())
            .filter_map(|t| t.value())
            .map(|t| t.trim().to_string())
            .find(|t| !t.is_empty())
    }

    /// 幂等键：顶层 `message_id`（服务端权威）→ `client_id`（对端生成）→ item `msg_id`
    ///
    /// 均缺失返回空串（调用方构造事件时应保证非空，否则自行生成占位 ID）。
    /// `client_id` 保留在链上是因为它同时是**出站本地幂等键**（见 `build_send_body`）。
    pub fn message_key(&self) -> String {
        if let Some(key) = self.message_id.as_ref().and_then(value_to_key) {
            return key;
        }
        if !self.client_id.is_empty() {
            return self.client_id.clone();
        }
        self.item_list
            .iter()
            .find_map(|item| item.msg_id.as_ref().and_then(value_to_key))
            .unwrap_or_default()
    }

    /// 平台侧消息 ID（顶层 `message_id` 优先，回落 `client_id`），用于 `external_key` 存档
    ///
    /// 与 [`Self::message_key`] 的区别：后者是**幂等键**（含 item 级兜底与「空串 =
    /// 需生成占位 ID」的契约）；本方法只回答「平台给这条消息的 ID 是什么」，
    /// 两者都没有时返回 `None`（不伪造）。
    pub fn platform_message_id(&self) -> Option<String> {
        self.message_id
            .as_ref()
            .and_then(value_to_key)
            .or_else(|| (!self.client_id.is_empty()).then(|| self.client_id.clone()))
    }
}

// ==================== AOP 入站事件 ====================

/// 微信 iLink 入站消息事件（AOP 信封）
///
/// DAO 侧长轮询 adapter 收到消息后 publish 此事件，由业务 consumer
/// （`ConsumeMode::Async`）异步消费——**读循环里不做业务**。
/// 信封只携带协议数据（channel_id + bot_id + 原始消息），身份与业务语义由消费侧补全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatInboundEvent {
    /// 消息归属的渠道 ID（长轮询按 channel 隔离，一个 bot 微信号 = 一个 channel）
    pub channel_id: String,
    /// bot 标识（order_key：同 bot 内串行、不同 bot 并行）
    pub bot_id: String,
    /// 幂等键（DAO 构造时从 message 解析，缺省时生成占位 ID）
    pub message_key: String,
    /// 本轮 `getupdates` 返回的新游标（服务端 **opaque** 值，只能原样回传）
    ///
    /// **P2 的载体**：轮询循环不再自己推进游标，而是把它随事件带出来；
    /// 只有消费者确认成功（DAL 的 `on_consumed`）后才真正推进 ——
    /// 上一轮没消费完时游标不动，下一轮会重拉同一批（`message_key` 幂等去重兜底），
    /// 因此「消费失败」不再等于「消息确定性丢失」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// iLink 原始消息
    pub message: IlinkMessage,
}

impl crate::pkg::aop::Event for WechatInboundEvent {
    fn kind(&self) -> common::enums::EventTopic {
        common::enums::EventTopic::WechatInboundMessage
    }

    fn id(&self) -> &str {
        &self.message_key
    }

    fn order_key(&self) -> &str {
        &self.bot_id
    }
}

// ==================== 单测 ====================
#[cfg(test)]
#[path = "wechat_tests.rs"]
mod tests;
