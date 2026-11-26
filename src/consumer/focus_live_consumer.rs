//! 专注浮层实时消费者（AOP Sync）
//!
//! 把「思考图骨架 / 轮次输出 / 工具开始与结束 / 小脑决策」投影成**客户端可见**
//! 的最小字段集，再按 `agent_id` 转发到进程内 broadcast，由 SSE handler 推给前端。
//!
//! 为什么是 Sync：入队会让实时图慢一拍；这里只做一次 JSON 字符串转发，
//! 没有阻塞 I/O，内联执行成本可忽略。
//!
//! 为什么在消费端做投影：AOP 事件是内部链路，带有 `organization_id` /
//! `user_id` / 模型标识 / 工具原始入参与结果等字段；SSE 是**对外出口**，
//! 必须在离开服务端前完成数据最小化（项目边界：内部不脱敏，出口脱敏）。

use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::pkg::request_context::AOP_CONTEXT_CARRIER_KEY;
use async_trait::async_trait;
use common::enums::EventTopic;
use common::error::Result;
use serde_json::Value;
use std::borrow::Cow;

pub struct FocusLiveConsumer;

impl FocusLiveConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FocusLiveConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for FocusLiveConsumer {
    fn name(&self) -> &str {
        "focus_live"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![
            Subscription::new(EventTopic::AgentThinkGraph),
            Subscription::new(EventTopic::AgentThinkOutput),
            Subscription::new(EventTopic::AgentToolStarted),
            Subscription::new(EventTopic::AgentToolExecuted),
            Subscription::new(EventTopic::AgentThinkRound),
            Subscription::new(EventTopic::AgentCerebellumDecided),
        ]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, _ctx: RequestContext, mut event: serde_json::Value) -> Result<()> {
        // 客户端边界：去掉仅服务端链路使用的上下文档（含 user/org/log 等内部标识）。
        // AOP 分发前已用它还原出 `_ctx`，这里删掉不影响任何消费方。
        if let Some(obj) = event.as_object_mut() {
            obj.remove(AOP_CONTEXT_CARRIER_KEY);
        }

        // 路由键：顶层 agent_id；工具完成事件由 `entry.agent_id` 冗余补全。
        let agent_id = event
            .get("agent_id")
            .and_then(Value::as_str)
            .or_else(|| event.pointer("/entry/agent_id").and_then(Value::as_str))
            .map(str::to_string);
        // 没有归属 Agent 的运行时事件无法路由，直接丢弃（不应发生）
        let Some(agent_id) = agent_id else {
            return Ok(());
        };

        sanitize_for_client(&mut event);

        // 已是 JSON：直接序列化一次字符串转发，避免反复解析 / 重排字段
        let json = serde_json::to_string(&event)
            .map_err(|e| common::error::Error::internal(format!("focus live serialize: {e}")))?;
        crate::pkg::focus_live::publish(&agent_id, json);
        Ok(())
    }
}

/// 出口投影：删除内部标识 + 对自由文本 / 工具载荷做凭证脱敏
///
/// 只保留前端实时图需要的字段；`metadata` 里可能带服务端路径等内部信息，
/// 整体不下发。脱敏规则与其它对外接口共用 `common::redaction::EXPORT`。
fn sanitize_for_client(event: &mut Value) {
    let Some(obj) = event.as_object_mut() else {
        return;
    };

    // 内部身份 / 计费维度：前端不需要，且属于跨租户敏感信息
    for key in [
        "organization_id",
        "user_id",
        "model_provider_id",
        "model_name",
    ] {
        obj.remove(key);
    }

    // 工具开始：模型自述意图与业务入参
    if let Some(args) = obj.get_mut("args") {
        common::redaction::redact_json(args);
    }
    redact_text_field(obj.get_mut("intent"));

    // 思考图 / 输出 / 小脑：自由文本里可能被模型回显凭证
    for key in ["stimulus", "reason", "error", "reasoning", "content"] {
        redact_text_field(obj.get_mut(key));
    }
    if let Some(answers) = obj.get_mut("answers").and_then(Value::as_array_mut) {
        for answer in answers {
            if let Some(summary) = answer.get_mut("summary") {
                redact_text_field(Some(summary));
            }
        }
    }

    // 工具完成：完整 entry 快照里只投影展示需要的字段
    if let Some(entry) = obj.get_mut("entry").and_then(Value::as_object_mut) {
        // metadata 是内部调用附注（caller_location 等），不对外
        entry.remove("metadata");
        if let Some(input) = entry.get_mut("input") {
            common::redaction::redact_json(input);
        }
        if let Some(output) = entry.get_mut("output") {
            common::redaction::redact_json(output);
        }
        redact_text_field(entry.get_mut("error"));
    }
}

/// 字符串字段就地脱敏；无敏感内容时不写回（避免制造一次无意义的内存分配）
fn redact_text_field(slot: Option<&mut Value>) {
    let Some(value) = slot else {
        return;
    };
    let Some(text) = value.as_str() else {
        return;
    };
    if let Cow::Owned(redacted) = common::redaction::redact_text(text) {
        *value = Value::String(redacted);
    }
}
