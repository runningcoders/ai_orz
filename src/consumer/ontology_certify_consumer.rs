//! Ontology certify consumer (AOP async)
//!
//! 订阅 `memory.terms.written` 事件，对本次落库的词条执行「写后认证 + 漂移记账」：
//! - OntologyDomain.certify_memory_terms：词表命中（Canonical / ViaSynonym）→
//!   is_published 置位（质量认证）；词表外（Drift）→ needs_review 软门禁，
//!   不拦截、不撤回
//! - OntologyDriftEvent → DuckDB：只记原文，解析结论不入库（不可变审计日志）
//!
//! 设计要点（design 决策 #16）：
//! - `ConsumeMode::Async`：异步消费，写路径（runtime 尾部 publish）不阻塞
//! - 消费者 → Domain 是合法方向（task_event_consumer.rs 先例）
//! - 统计打点在消费侧完成（think_round_stats_consumer.rs 先例）

use async_trait::async_trait;

use crate::models::events::MemoryTermsWrittenEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::pkg::stats::{OntologyDriftEvent, global_stats};
use common::enums::EventTopic;
use common::error::Result;
use common::ontology::ResolvedTerm;

pub struct OntologyCertifyConsumer;

impl OntologyCertifyConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OntologyCertifyConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for OntologyCertifyConsumer {
    fn name(&self) -> &str {
        "ontology_certify"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::MemoryTermsWritten)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        // 反序列化失败 = 毒消息：映射 InvalidRequest（命中 PERMANENT_CODES 首次 Discard，
        // internal 会无限 Retry），错误细节已由框架 registry sys_error! 落日志
        let event: MemoryTermsWrittenEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::bad_request(format!(
                "failed to deserialize MemoryTermsWrittenEvent: {}",
                e
            ))
        })?;

        if event.terms.is_empty() {
            return Ok(());
        }

        // 同批去重：重复 (kind, raw_term) 只认证 / 打点一次，避免漂移计数虚高（保序）
        let mut seen = std::collections::HashSet::new();
        let terms = event
            .terms
            .iter()
            .map(|t| (t.kind, t.raw_term.clone()))
            .filter(|t| seen.insert(t.clone()))
            .collect::<Vec<_>>();

        // 写后认证：词表命中置位 is_published，词表外标记 needs_review（软门禁）
        let report = crate::service::domain::hr::domain()
            .ontology_domain()
            .certify_memory_terms(ctx.clone(), event.agent_id.clone(), terms)
            .await?;

        // 漂移记账：Drift 分支才打点，只记原文（解析结论不入库）
        if let Some(stats) = global_stats() {
            for entry in &report.entries {
                let ResolvedTerm::Drift { raw_term } = &entry.verdict else {
                    continue;
                };
                let stats_event = OntologyDriftEvent::new(event.created_at)
                    .with_agent_id(event.agent_id.clone().unwrap_or_default())
                    .with_kind(entry.kind.as_str().to_string())
                    .with_raw_term(raw_term.clone());
                // 打点失败不阻塞认证主流程，但必须留痕（失败仅在 lock 中毒 /
                // 表未注册 / flush 异常等基础设施故障时发生）
                if let Err(e) = stats.record(ctx.clone(), stats_event).await {
                    log_warn!(&ctx, "ontology_certify", "drift stats record failed: {}", e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 订阅契约：恰好订阅 memory.terms.written，Async 模式（不阻塞写路径）
    #[test]
    fn subscribes_memory_terms_written_in_async_mode() {
        let consumer = OntologyCertifyConsumer::new();
        assert_eq!(consumer.name(), "ontology_certify");
        let subs = consumer.subscriptions();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].kind, EventTopic::MemoryTermsWritten);
        assert!(matches!(consumer.consume_mode(), ConsumeMode::Async));
    }
}
