use crate::pkg::aop::Event;
use common::enums::EventTopic;
use common::ontology::TermKind;
use serde::{Deserialize, Serialize};

/// 单个写入词条（种类 + 原文）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrittenTerm {
    /// 词条种类（class = 节点类型，relation = 关系类型）
    pub kind: TermKind,
    /// 词元原文（trim 后，不解析、不翻译）
    pub raw_term: String,
}

/// 记忆词条写入事件（长期知识节点 / 关系原文落库后发布）
///
/// 写路径单管道（design 决策 #16）：runtime 沉淀链路尾部唯一动作是
/// [`crate::pkg::aop::publish`] 本事件——不打点、不解析、不调 domain/dal；
/// 写后认证与漂移记账全在消费侧（OntologyCertifyConsumer，Async 模式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTermsWrittenEvent {
    pub event_id: String,
    /// 沉淀方 Agent（记忆属主）；系统链路无归属时为 None
    pub agent_id: Option<String>,
    /// 本次落库的词条清单
    pub terms: Vec<WrittenTerm>,
    pub created_at: i64,
}

impl MemoryTermsWrittenEvent {
    pub fn new(agent_id: Option<String>, terms: Vec<WrittenTerm>) -> Self {
        Self {
            event_id: uuid::Uuid::now_v7().to_string(),
            agent_id,
            terms,
            created_at: common::constants::utils::current_timestamp_ms(),
        }
    }
}

impl Event for MemoryTermsWrittenEvent {
    fn kind(&self) -> EventTopic {
        EventTopic::MemoryTermsWritten
    }

    fn id(&self) -> &str {
        &self.event_id
    }

    fn order_key(&self) -> &str {
        self.agent_id.as_deref().unwrap_or("")
    }

    fn created_at(&self) -> i64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 序列化往返：kind 线格式为小写（"relation"/"class"），原文保真
    #[test]
    fn serde_roundtrip() {
        let event = MemoryTermsWrittenEvent::new(
            Some("agent_1".to_string()),
            vec![
                WrittenTerm {
                    kind: TermKind::Relation,
                    raw_term: "管理".to_string(),
                },
                WrittenTerm {
                    kind: TermKind::Class,
                    raw_term: "preference".to_string(),
                },
            ],
        );
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["terms"][0]["kind"], "relation");
        assert_eq!(json["terms"][1]["kind"], "class");
        assert_eq!(json["terms"][0]["raw_term"], "管理");

        let back: MemoryTermsWrittenEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back.terms.len(), 2);
        assert_eq!(back.terms[0].raw_term, "管理");
        assert!(matches!(back.terms[0].kind, TermKind::Relation));
        assert!(matches!(back.terms[1].kind, TermKind::Class));
        assert_eq!(back.agent_id.as_deref(), Some("agent_1"));
    }
}
