//! AOP 事件主题（topic）—— 前后端共享的唯一 SSOT
//!
//! 取代原先的 `pkg::aop::EventKind(pub &'static str)`：一个没有闭集约束的裸字符串，
//! 拼错一个字母的后果是「查不到订阅者 → 静默丢弃」（`Registry::publish` 查不到订阅者
//! 时直接 return、连日志都不打）。改为闭合枚举后，后端拼错 topic 变成**编译错误**；
//! 前端 AOP 监控页的筛选也从「手打字符串」变成遍历 [`EventTopic::ALL`] 的下拉。
//!
//! ## 不变量
//! - **一个主题对应一个生产者**（1 topic : 1 producer）；消费者可订阅多个。
//! - 枚举只保证「值合法」，**不保证「只用一次」**——两个生产者声明同一 topic 在类型上
//!   仍然合法，故注册期仍需运行期占用校验。
//!
//! ## 线格式
//! [`EventTopic::as_str`] 与历史字符串**逐字一致**（如 `"message.created"`）；注入事件
//! JSON 的 `kind` 字段、`AopEventMeta.event_kind`、前端展示全部零变化。
//! 字符串字面量只在 `as_str` / `parse` 各出现一次，避免两处各写一遍而漂移。
//!
//! ## 严格 / 宽容的分工
//! - **请求侧严格**：查询参数用 `Option<EventTopic>`。
//! - **响应侧保持 `String`**：闭合枚举一旦放进响应，后端新增 topic 而前端未同步重建
//!   就会**整页反序列化失败**。展示层用 [`EventTopic::parse`] 映射标签，`None` 原样显示。

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Metadata, Schema, SchemaObject};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// AOP 事件主题（topic）
///
/// 后端用于事件路由与「消费收尾回调归属」反查，前端用于监控页的筛选项与展示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventTopic {
    /// `message.created`
    MessageCreated,
    /// `agent.settle.requested`
    AgentSettleRequested,
    /// `cron.trigger`
    CronTrigger,
    /// `agent.loop`
    AgentLoop,
    /// `agent.think.round`
    AgentThinkRound,
    /// `agent.tool.executed`
    AgentToolExecuted,
    /// `agent.state.changed`（当前无消费者：P5，见设计稿 §8）
    AgentStateChanged,
    /// `task.status_changed`
    TaskStatusChanged,
    /// `organization.changed`
    OrganizationChanged,
    /// `email.inbound.message`
    EmailInboundMessage,
    /// `lark.inbound.message`
    LarkInboundMessage,
    /// `wechat.inbound.message`
    WechatInboundMessage,
    /// `federation.outbound`
    FederationOutbound,
    /// `federation.inbound.send_task`
    FederationInboundSendTask,
    /// `federation.inbound.other`（兜底 kind，当前无消费者）
    FederationInboundOther,
    /// `a2a.poll.requested`（A2A 轮询生产者派发的认领事件）
    A2aPollRequested,
}

impl EventTopic {
    /// 全部成员 —— 前端渲染筛选项直接遍历它，不再手打字符串
    pub const ALL: &'static [EventTopic] = &[
        EventTopic::MessageCreated,
        EventTopic::AgentSettleRequested,
        EventTopic::CronTrigger,
        EventTopic::AgentLoop,
        EventTopic::AgentThinkRound,
        EventTopic::AgentToolExecuted,
        EventTopic::AgentStateChanged,
        EventTopic::TaskStatusChanged,
        EventTopic::OrganizationChanged,
        EventTopic::EmailInboundMessage,
        EventTopic::LarkInboundMessage,
        EventTopic::WechatInboundMessage,
        EventTopic::FederationOutbound,
        EventTopic::FederationInboundSendTask,
        EventTopic::FederationInboundOther,
        EventTopic::A2aPollRequested,
    ];

    /// 线格式：与历史 `EventKind` 的字符串**逐字一致**
    ///
    /// 注入事件 JSON 的 `kind` 字段、`AopEventMeta.event_kind` 都取它。
    pub const fn as_str(&self) -> &'static str {
        match self {
            EventTopic::MessageCreated => "message.created",
            EventTopic::AgentSettleRequested => "agent.settle.requested",
            EventTopic::CronTrigger => "cron.trigger",
            EventTopic::AgentLoop => "agent.loop",
            EventTopic::AgentThinkRound => "agent.think.round",
            EventTopic::AgentToolExecuted => "agent.tool.executed",
            EventTopic::AgentStateChanged => "agent.state.changed",
            EventTopic::TaskStatusChanged => "task.status_changed",
            EventTopic::OrganizationChanged => "organization.changed",
            EventTopic::EmailInboundMessage => "email.inbound.message",
            EventTopic::LarkInboundMessage => "lark.inbound.message",
            EventTopic::WechatInboundMessage => "wechat.inbound.message",
            EventTopic::FederationOutbound => "federation.outbound",
            EventTopic::FederationInboundSendTask => "federation.inbound.send_task",
            EventTopic::FederationInboundOther => "federation.inbound.other",
            EventTopic::A2aPollRequested => "a2a.poll.requested",
        }
    }

    /// 反向解析（`AopEventMeta.event_kind` 是 `String`）
    ///
    /// 未知返回 `None` = 无生产者 = ①类纯通知（语义与改造前一致）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "message.created" => Some(EventTopic::MessageCreated),
            "agent.settle.requested" => Some(EventTopic::AgentSettleRequested),
            "cron.trigger" => Some(EventTopic::CronTrigger),
            "agent.loop" => Some(EventTopic::AgentLoop),
            "agent.think.round" => Some(EventTopic::AgentThinkRound),
            "agent.tool.executed" => Some(EventTopic::AgentToolExecuted),
            "agent.state.changed" => Some(EventTopic::AgentStateChanged),
            "task.status_changed" => Some(EventTopic::TaskStatusChanged),
            "organization.changed" => Some(EventTopic::OrganizationChanged),
            "email.inbound.message" => Some(EventTopic::EmailInboundMessage),
            "lark.inbound.message" => Some(EventTopic::LarkInboundMessage),
            "wechat.inbound.message" => Some(EventTopic::WechatInboundMessage),
            "federation.outbound" => Some(EventTopic::FederationOutbound),
            "federation.inbound.send_task" => Some(EventTopic::FederationInboundSendTask),
            "federation.inbound.other" => Some(EventTopic::FederationInboundOther),
            "a2a.poll.requested" => Some(EventTopic::A2aPollRequested),
            _ => None,
        }
    }
}

impl std::fmt::Display for EventTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// 手写 Serialize/Deserialize：复用 as_str()/parse()，保证线格式与字符串字面量同源
// （用 #[serde(rename = ...)] 就得为每个变体再写一遍，两处会漂移）。
impl Serialize for EventTopic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EventTopic {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EventTopicVisitor;

        impl serde::de::Visitor<'_> for EventTopicVisitor {
            type Value = EventTopic;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a valid AOP event topic string (e.g. \"message.created\")")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<EventTopic, E> {
                EventTopic::parse(v)
                    .ok_or_else(|| E::custom(format!("unknown AOP event topic: {v}")))
            }
        }

        deserializer.deserialize_str(EventTopicVisitor)
    }
}

// 手写 JsonSchema：derive 会按变体名生成 "MessageCreated" 这类枚举值，
// 与真实线格式（点分字符串）不一致；这里直接声明为字符串 + 线格式枚举值。
impl JsonSchema for EventTopic {
    fn schema_name() -> String {
        "EventTopic".to_string()
    }

    fn json_schema(_gen: &mut SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(
                EventTopic::ALL
                    .iter()
                    .map(|t| serde_json::Value::String(t.as_str().to_string()))
                    .collect(),
            ),
            metadata: Some(Box::new(Metadata {
                description: Some(
                    "AOP 事件主题（线格式为点分字符串，如 \"message.created\"）".to_string(),
                ),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// as_str / parse 互为逆运算，且 ALL 与两者一致（防止新增变体时漏改一处）
    #[test]
    fn as_str_and_parse_are_inverse() {
        for topic in EventTopic::ALL {
            assert_eq!(
                EventTopic::parse(topic.as_str()),
                Some(*topic),
                "parse 无法还原 {}",
                topic.as_str()
            );
        }
        assert_eq!(EventTopic::parse("no.such.topic"), None);
        assert_eq!(EventTopic::parse(""), None);
    }

    /// 线格式不得漂移（与历史 EventKind 字符串逐字一致）
    #[test]
    fn wire_format_is_stable() {
        assert_eq!(EventTopic::MessageCreated.as_str(), "message.created");
        assert_eq!(
            EventTopic::AgentSettleRequested.as_str(),
            "agent.settle.requested"
        );
        assert_eq!(EventTopic::CronTrigger.as_str(), "cron.trigger");
        assert_eq!(
            EventTopic::FederationInboundSendTask.as_str(),
            "federation.inbound.send_task"
        );
        assert_eq!(EventTopic::A2aPollRequested.as_str(), "a2a.poll.requested");
    }

    /// 未知字符串反序列化失败（保护「拼错 → 静默丢弃」不再发生）
    #[test]
    fn deserialize_rejects_unknown_topic() {
        let ok: EventTopic = serde_json::from_value(serde_json::json!("message.created")).unwrap();
        assert_eq!(ok, EventTopic::MessageCreated);
        let err = serde_json::from_value::<EventTopic>(serde_json::json!("message.creted"));
        assert!(err.is_err());
    }
}
