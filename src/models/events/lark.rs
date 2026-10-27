//! 飞书事件订阅类型定义（AOP 事件统一目录）
//!
//! 覆盖 `im.message.receive_v1` 事件结构（P2P 私信场景）与 AOP 入站事件信封。
//! 事件类型为纯数据（serde DTO + Event impl），归属 models 层；
//! DAO 侧长连接 adapter 只负责收帧解析后 publish，消费在 `consumer/lark_inbound`。
//! 字段参考：https://open.feishu.cn/document/event/im.message.receive_v1

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `im.message.receive_v1` 事件顶层包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkMessageEvent {
    /// schema 版本
    #[serde(default)]
    pub schema: String,
    /// 事件头
    pub header: LarkEventHeader,
    /// 事件数据
    pub event: LarkMessageEventData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEventHeader {
    /// 事件唯一 ID（用于幂等去重）
    pub event_id: String,
    /// 事件类型，例如 "im.message.receive_v1"
    pub event_type: String,
    /// 事件创建时间（毫秒字符串）
    pub create_time: String,
    /// Verification Token（用于事件校验）
    #[serde(default)]
    pub token: String,
    /// 应用 ID
    #[serde(default)]
    pub app_id: String,
    /// 租户 key
    #[serde(default)]
    pub tenant_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkMessageEventData {
    pub sender: LarkEventSender,
    pub message: LarkEventMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEventSender {
    pub sender_id: LarkSenderId,
    /// 发送者 ID 类型，例如 "open_id"
    #[serde(default)]
    pub sender_type: String,
}

/// null 容错的字符串反序列化：字段显式为 `null` 时按空串处理
///
/// 飞书是外部 API 边界：应用无 user_id/union_id 权限时会下发 `"user_id": null`
/// 而非省略字段，此时 `#[serde(default)]` 不生效（它只补缺失字段），
/// 直接反序列化到 `String` 会整帧失败，消息静默丢失。
fn null_as_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkSenderId {
    /// 用户的 open_id（主要标识）
    #[serde(default, deserialize_with = "null_as_empty")]
    pub open_id: String,
    /// 用户的 user_id（企业内；应用无权限时飞书下发 null）
    #[serde(default, deserialize_with = "null_as_empty")]
    pub user_id: String,
    /// 用户的 union_id（应用无权限时飞书下发 null）
    #[serde(default, deserialize_with = "null_as_empty")]
    pub union_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEventMessage {
    /// 消息 ID
    pub message_id: String,
    /// 根消息 ID（话题消息时存在）
    #[serde(default)]
    pub root_id: Option<String>,
    /// 父消息 ID（回复消息时存在）
    #[serde(default)]
    pub parent_id: Option<String>,
    /// 话题 ID（`omt_` 前缀，仅话题消息存在）
    ///
    /// 飞书「话题」形态的唯一标识：同一话题内的所有消息共享同一 `thread_id`。
    /// 普通回复/私信无此字段，故为 `None`。
    #[serde(default)]
    pub thread_id: Option<String>,
    /// 消息创建时间（毫秒字符串）
    pub create_time: String,
    /// 会话 ID
    #[serde(default)]
    pub chat_id: String,
    /// 会话类型：`p2p`（私信）或 `group`（群聊）
    #[serde(default)]
    pub chat_type: String,
    /// 消息类型：`text`/`image`/`post`/`file` 等，本期处理 `text`、`image` 与 `post`
    #[serde(default)]
    pub message_type: String,
    /// 消息内容（JSON 字符串）
    ///
    /// 文本消息：`{"text":"消息内容"}`
    /// 图片消息：`{"image_key":"img_v3_xxx"}`
    /// 富文本消息：`{"title":"...","content":[[{"tag":"text",...},{"tag":"img",...}]]}`
    /// 或按语言包一层（`{"zh_cn":{...}}`）
    #[serde(default)]
    pub content: String,
    /// 消息原始扩展字段（保留未识别字段）
    #[serde(default)]
    pub extra: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkTextContent {
    #[serde(default)]
    pub text: String,
}

/// 图片消息内容（`message_type == "image"`）
///
/// 飞书图片消息 content 形如 `{"image_key":"img_v3_xxx"}`，只携带资源键；
/// 实际字节需用事件里的 `message_id` + image_key 调资源下载接口获取。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LarkImageContent {
    #[serde(default)]
    pub image_key: String,
}

/// 富文本消息（`message_type == "post"`）解析结果
///
/// 飞书富文本是**图文混排**的唯一单条形态（纯文本/纯图片各有独立 message_type）。
/// 解析后正文与图片资源键分离：正文作为内部消息内容，图片走附件链路。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LarkPostParsed {
    /// 正文（标题 + 各段落，段落间以换行分隔）
    pub text: String,
    /// 图片资源键（按出现顺序，已去重）
    pub image_keys: Vec<String>,
}

// ==================== AOP 入站事件 ====================

/// 飞书 WS 入站消息事件（AOP 信封）
///
/// DAO 侧长连接 adapter 收到 `im.message.receive_v1` 后 publish 此事件，
/// 由业务 consumer（`ConsumeMode::Async`）异步消费——**读循环里不做业务**。
/// 信封只携带协议数据（app_id + 原始事件），身份与业务语义由消费侧补全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkInboundEvent {
    /// 事件归属的飞书应用（多应用路由依据）
    pub app_id: String,
    /// 飞书原始事件
    pub event: LarkMessageEvent,
}

impl crate::pkg::aop::Event for LarkInboundEvent {
    fn kind(&self) -> common::enums::EventTopic {
        common::enums::EventTopic::LarkInboundMessage
    }

    fn id(&self) -> &str {
        &self.event.header.event_id
    }

    fn order_key(&self) -> &str {
        // 同应用内按到达顺序串行处理；不同应用并行
        &self.app_id
    }

    fn created_at(&self) -> i64 {
        self.event.header.create_time.parse().unwrap_or(0)
    }
}

impl LarkMessageEvent {
    /// 是否为 P2P 私信
    pub fn is_p2p(&self) -> bool {
        self.event.message.chat_type == "p2p"
    }

    /// 是否为群聊消息
    pub fn is_group(&self) -> bool {
        self.event.message.chat_type == "group"
    }

    /// 是否为文本消息
    pub fn is_text(&self) -> bool {
        self.event.message.message_type == "text"
    }

    /// 是否为图片消息
    pub fn is_image(&self) -> bool {
        self.event.message.message_type == "image"
    }

    /// 是否为富文本消息（图文混排）
    pub fn is_post(&self) -> bool {
        self.event.message.message_type == "post"
    }

    /// 解析文本消息内容
    pub fn parse_text(&self) -> Option<String> {
        if !self.is_text() {
            return None;
        }
        let parsed: LarkTextContent = serde_json::from_str(&self.event.message.content).ok()?;
        Some(parsed.text)
    }

    /// 解析图片消息的 `image_key`（非图片消息或内容缺失时返回 `None`）
    pub fn parse_image_key(&self) -> Option<String> {
        if !self.is_image() {
            return None;
        }
        let parsed: LarkImageContent = serde_json::from_str(&self.event.message.content).ok()?;
        let key = parsed.image_key.trim();
        (!key.is_empty()).then(|| key.to_string())
    }

    /// 解析富文本消息（非 `post` 或内容非法时返回 `None`）
    ///
    /// 飞书 `post` 的 content 有两种下发形态：直挂 `content` 数组，或按语言
    /// 包一层（`zh_cn` / `en_us` …）；本方法统一归一后抽取正文与图片键。
    /// 支持的标签：`text` / `a`（取 text，缺失用 href）/ `at`（`@用户`）/ `img`。
    pub fn parse_post(&self) -> Option<LarkPostParsed> {
        if !self.is_post() {
            return None;
        }
        let value: Value = serde_json::from_str(&self.event.message.content).ok()?;
        let body = locate_post_body(&value)?;

        let mut lines: Vec<String> = Vec::new();
        if let Some(title) = body
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
        {
            lines.push(title.to_string());
        }

        let mut image_keys: Vec<String> = Vec::new();
        for paragraph in body.get("content")?.as_array()? {
            let mut line = String::new();
            for element in paragraph.as_array().into_iter().flatten() {
                match element.get("tag").and_then(Value::as_str) {
                    Some("text") => line.push_str(
                        element
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    ),
                    Some("a") => {
                        // 链接：展示锚文本，缺失时退回 href
                        let label = element
                            .get("text")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .or_else(|| element.get("href").and_then(Value::as_str));
                        if let Some(label) = label {
                            line.push_str(label);
                        }
                    }
                    Some("at") => {
                        // 提及：优先展示用户名，缺失时退回 user_id
                        let name = element
                            .get("user_name")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|name| !name.is_empty())
                            .or_else(|| element.get("user_id").and_then(Value::as_str));
                        if let Some(name) = name {
                            line.push('@');
                            line.push_str(name);
                        }
                    }
                    Some("img") => {
                        if let Some(key) = element
                            .get("image_key")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|key| !key.is_empty())
                            && !image_keys.iter().any(|existing| existing == key)
                        {
                            image_keys.push(key.to_string());
                        }
                    }
                    _ => {}
                }
            }
            let line = line.trim();
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }

        Some(LarkPostParsed {
            text: lines.join("\n"),
            image_keys,
        })
    }

    /// 发送者 open_id
    pub fn sender_open_id(&self) -> &str {
        &self.event.sender.sender_id.open_id
    }
}

/// 定位富文本正文：兼容「直挂 content 数组」与「按语言包一层」两种下发形态
///
/// 语言优先级 `zh_cn` → `en_us` → 任意含 `content` 数组的语言。
fn locate_post_body(value: &Value) -> Option<&Value> {
    let has_content = |body: &Value| body.get("content").and_then(Value::as_array).is_some();
    if has_content(value) {
        return Some(value);
    }
    let object = value.as_object()?;
    for locale in ["zh_cn", "en_us"] {
        if let Some(body) = object.get(locale)
            && has_content(body)
        {
            return Some(body);
        }
    }
    object.values().find(|body| has_content(body))
}
#[cfg(test)]
#[path = "lark_tests.rs"]
mod tests;
