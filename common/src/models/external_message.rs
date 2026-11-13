//! 外部渠道入站消息上下文（渠道无关聚合）
//!
//! 把「消息来自哪条外部消息 / 哪个外部会话 / 提到了谁」这类跨渠道通用信息
//! 收敛到一个结构里。新增渠道只需填充本结构，无需给 `AdaptedMessage`、
//! `SendToAgentCommand` 逐字段加参数；后续新增外部元数据也只改这里，
//! 不波及所有命令构造点。

use crate::mention::ExternalMention;

/// 外部渠道入站消息的元数据
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalMessageContext {
    /// 平台侧消息 ID（如 `lark:om_xxx`），渠道前缀防撞键
    pub external_key: Option<String>,
    /// 平台侧话题/线程 ID（如飞书 `omt_xxx`）
    pub thread_id: Option<String>,
    /// 平台侧会话 ID（飞书私聊/群聊均有 `oc_xxx`）
    ///
    /// 渠道无关语义：**消息所属会话**。入站用于会话绑定与出站兜底发送。
    pub chat_id: Option<String>,
    /// 平台侧 @ 提及原始信息
    ///
    /// 命中本系统 Agent 的提及已在正文替换为内部提及协议；未命中的保留
    /// `@name` 展示。无论是否命中，平台原始标识都在这里完整保留。
    pub external_mentions: Vec<ExternalMention>,
}

impl ExternalMessageContext {
    /// 是否没有任何外部元数据
    pub fn is_empty(&self) -> bool {
        self.external_key.is_none()
            && self.thread_id.is_none()
            && self.chat_id.is_none()
            && self.external_mentions.is_empty()
    }
}
