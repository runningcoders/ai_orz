//! 邮件渠道 IMAP 入站事件类型定义（AOP 事件统一目录）
//!
//! 覆盖 IMAP 轮询拉取并经 MIME 解析后的入站邮件事件信封。
//! 事件类型为纯数据（serde DTO + Event impl），归属 models 层；
//! DAO 侧受管轮询（`dao/email/imap.rs`）按邮箱凭证（credential）拉取新邮件、
//! 解析出主题与纯文本正文后 publish，消费在 `consumer/email_inbound`。
//!
//! 轮询单元 = 一个代理邮箱（EmailBot 凭证），而非单个渠道：
//! 同一邮箱可能被 N 个渠道共用，收件人 From 与 `email_to_address` 的
//! 二维匹配（路由）由消费侧 DAL 完成——信封只携带协议数据。

use serde::{Deserialize, Serialize};

/// 邮件 IMAP 入站消息事件（AOP 信封）
///
/// DAO 侧受管轮询收到新邮件后 publish 此事件，由业务 consumer
/// （`ConsumeMode::Async`）异步消费——**读循环里不做业务**。
/// 信封只携带解析后的协议数据（credential_id + from + 主题/正文），
/// 身份与业务语义（渠道匹配、去重）由消费侧补全。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailInboundEvent {
    /// 邮件归属的邮箱凭证 ID（order_key：同邮箱内串行、不同邮箱并行）
    pub credential_id: String,
    /// 发件人地址（DAO 侧已规范化为小写，供二维路由匹配 `email_to_address`）
    pub from: String,
    /// 幂等键：RFC Message-ID（去 `<>` 包裹；缺失时以内容哈希兜底，DAO 构造时保证非空）
    pub message_key: String,
    /// 邮件主题（RFC 2047 解码后）
    pub subject: String,
    /// 纯文本正文（multipart 取 text/plain，缺失时降级 text/html 剥标签）
    pub content: String,
    /// IMAP UID（仅用于日志追踪与排序参考，不参与幂等判断）
    pub uid: u32,
}

impl EmailInboundEvent {
    /// 外部键前缀：出站回写与入站去重共用同一命名空间
    pub const EXTERNAL_KEY_PREFIX: &'static str = "email";

    /// 落库幂等键：`email:<Message-ID>`（与出站回写 external_key 格式对齐）
    pub fn external_key(&self) -> String {
        format!("{}:{}", Self::EXTERNAL_KEY_PREFIX, self.message_key)
    }
}

impl crate::pkg::aop::Event for EmailInboundEvent {
    fn kind(&self) -> crate::pkg::aop::EventKind {
        crate::pkg::aop::EventKind::new("email.inbound.message")
    }

    fn id(&self) -> &str {
        &self.message_key
    }

    fn order_key(&self) -> &str {
        &self.credential_id
    }
}

// ==================== 单测 ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> EmailInboundEvent {
        EmailInboundEvent {
            credential_id: "cred_email_1".to_string(),
            from: "peer@example.com".to_string(),
            message_key: "<2026091401@example.com>".to_string(),
            subject: "周报同步".to_string(),
            content: "本周进展如下……".to_string(),
            uid: 42,
        }
    }

    /// AOP 信封：Event 语义（kind/id/order_key）
    #[test]
    fn test_email_inbound_event_semantics() {
        use crate::pkg::aop::Event;
        let event = sample_event();
        assert_eq!(
            event.kind(),
            crate::pkg::aop::EventKind::new("email.inbound.message")
        );
        assert_eq!(event.id(), "<2026091401@example.com>");
        assert_eq!(event.order_key(), "cred_email_1");
    }

    /// Value 往返（AOP 队列以 serde_json::Value 传输）
    #[test]
    fn test_email_inbound_event_serde_roundtrip() {
        let event = sample_event();
        let value = serde_json::to_value(&event).unwrap();
        let back: EmailInboundEvent = serde_json::from_value(value).unwrap();
        assert_eq!(back.credential_id, event.credential_id);
        assert_eq!(back.from, event.from);
        assert_eq!(back.message_key, event.message_key);
        assert_eq!(back.subject, event.subject);
        assert_eq!(back.content, event.content);
        assert_eq!(back.uid, event.uid);
    }

    /// 外部键格式：`email:<Message-ID>`，与出站回写对齐
    #[test]
    fn test_external_key_format() {
        let event = sample_event();
        assert_eq!(event.external_key(), "email:<2026091401@example.com>");
    }
}
