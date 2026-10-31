//! 实体属性（ABox 泛化挂载）：带本体门闩的写入 / 生效属性读取 / 按属性值召回

use super::MemoryDalImpl;
use crate::models::memory::KnowledgePropertyPo;
use crate::models::ontology::OntologyPropertyPo;
use crate::pkg::RequestContext;
use common::enums::ontology::{Cardinality, PropertyTargetType};
use common::error::{Result, bail_err};
use common::ontology::normalize;
use common::ontology_property::{
    PropertyEntry, PropertyFilter, PropertyIssue, PropertySaveReport, PropertyValueIssue,
    PropertyWriteIssue, ValueRange, allows_append, validate_property_value,
};
use std::collections::{HashMap, HashSet};

impl MemoryDalImpl {
    pub(super) async fn save_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
        entries: Vec<PropertyEntry>,
    ) -> Result<PropertySaveReport> {
        if entries.is_empty() {
            return Ok(PropertySaveReport::default());
        }

        // 词表很小，既有做法就是全量加载 + 内存映射（与 load_lexicon 同口径），
        // 不做「按 applies_to 过滤的 SQL 查询」
        let defs = self.ontology_dao.list_all_properties(ctx.clone()).await?;
        let def_map: HashMap<String, OntologyPropertyPo> =
            defs.into_iter().map(|d| (d.term_key.clone(), d)).collect();

        // 基数判定需要知道「该槽位以外已占了几条」（IO，故在 DAL 取）
        let existing = self
            .memory_dao
            .list_active_properties(ctx.clone(), target_type, target_id)
            .await?;

        // 门闩 1.5 的判定原料：目标节点的实体类（node_type）。
        // 整批 entries 写的是同一个 target_id，故**循环外只查一次**，不让 IO 随
        // 条数放大。载体是边时没有实体类；节点查不到（创建期 / 已删）也没得判
        // ——两种情形一律取 None，后续跳过判定，不误报。
        let node_type = if target_type == PropertyTargetType::Node {
            self.memory_dao
                .get_knowledge_node(ctx.clone(), target_id)
                .await?
                .map(|node| node.node_type)
        } else {
            None
        };

        let mut issues: Vec<PropertyIssue> = Vec::new();
        let mut to_write: Vec<KnowledgePropertyPo> = Vec::new();
        // 批内占位：同一批次重复写同一属性也要计入基数
        let mut batch_taken: HashMap<String, usize> = HashMap::new();

        for entry in entries {
            let term_key = normalize(&entry.term_key);
            if term_key.is_empty() {
                // 空锚点无处挂载，跳过（防御性分支：写进去会成为查不到的孤儿行）
                log_warn!(
                    ctx,
                    "ontology_property",
                    "属性 term_key 归一化后为空，跳过写入: {:?}",
                    entry.term_key
                );
                continue;
            }

            // —— 门闩 0：本体是否已定义该属性 ——
            // 未定义 ⇒ 按值自身类型兜底写入（不丢数据），只记告警：
            // 术语漂移是常态，写侧不裁决认知分歧
            let (data_type, cardinality, range, target_allowed) = match def_map.get(&term_key) {
                Some(def) => (
                    def.data_type(),
                    def.cardinality(),
                    def.value_range(),
                    def.allows_target(target_type),
                ),
                None => {
                    issues.push(PropertyIssue {
                        term_key: term_key.clone(),
                        kind: PropertyWriteIssue::UndefinedProperty,
                    });
                    (
                        entry.value.data_type(),
                        Cardinality::Single,
                        ValueRange::default(),
                        true,
                    )
                }
            };

            // —— 门闩 1：载体 ——（软门禁，记录后照写）
            if !target_allowed {
                issues.push(PropertyIssue {
                    term_key: term_key.clone(),
                    kind: PropertyWriteIssue::TargetNotAllowed,
                });
            }

            // —— 门闩 1.5：实体类域（`domain_classes`）——（软门禁，记录后照写）
            // 「属性挂错实体类」多半是声明漏了或节点类型填错，属认知分歧，
            // 与载体不符同口径处理：只记账、不拦写。未定义属性（门闩 0 已记过一条）
            // 与无法判定（节点不存在 / 载体是边）一律跳过，避免重复或空告警。
            let domain_violation = match (def_map.get(&term_key), node_type.as_deref()) {
                (Some(def), Some(node_type)) if !def.allows_domain_class(node_type) => {
                    Some((node_type.to_string(), def.parse_domain_classes()))
                }
                _ => None,
            };
            if let Some((node_type, domain_classes)) = domain_violation {
                issues.push(PropertyIssue {
                    term_key: term_key.clone(),
                    kind: PropertyWriteIssue::DomainClassNotAllowed {
                        node_type,
                        domain_classes,
                    },
                });
            }

            // —— 门闩 2 + 3：类型（硬拦）与值域（软门禁）——
            if let Err(value_issue) = validate_property_value(data_type, &entry.value, &range) {
                let kind = match value_issue {
                    PropertyValueIssue::TypeMismatch { expected } => {
                        PropertyWriteIssue::TypeMismatch { expected }
                    }
                    PropertyValueIssue::NumberOutOfRange => PropertyWriteIssue::NumberOutOfRange,
                    PropertyValueIssue::NotInEnumValue => PropertyWriteIssue::NotInEnumValue,
                };
                let rejects = kind.rejects_write();
                issues.push(PropertyIssue {
                    term_key: term_key.clone(),
                    kind,
                });
                if rejects {
                    continue;
                }
            }

            // —— 门闩 4：基数 ——
            // 单值属性强制槽位 0：新值取代旧值正是版本化修正的本意，不算冲突
            let ordinal = match cardinality {
                Cardinality::Single => 0,
                Cardinality::Many => entry.ordinal,
            };
            let occupied_other = existing
                .iter()
                .filter(|p| p.term_key == term_key && p.ordinal != ordinal)
                .count()
                + batch_taken.get(&term_key).copied().unwrap_or(0);
            if !allows_append(cardinality, occupied_other) {
                issues.push(PropertyIssue {
                    term_key: term_key.clone(),
                    kind: PropertyWriteIssue::CardinalityExceeded,
                });
                continue;
            }
            *batch_taken.entry(term_key.clone()).or_insert(0) += 1;

            to_write.push(KnowledgePropertyPo::new(
                target_type,
                target_id.to_string(),
                term_key,
                entry.value,
                ordinal,
            ));
        }

        if to_write.is_empty() {
            return Ok(PropertySaveReport {
                accepted: 0,
                issues,
            });
        }

        // 整批一个事务：不留「一半新版一半旧版」的中间态
        self.memory_dao
            .batch_upsert_properties(ctx.clone(), &to_write)
            .await?;

        if !issues.is_empty() {
            log_warn!(
                ctx,
                "ontology_property",
                "实体属性写入 {} 条，{} 条有判定问题（{} 条被拒）",
                to_write.len(),
                issues.len(),
                issues.iter().filter(|i| i.kind.rejects_write()).count()
            );
        }

        Ok(PropertySaveReport {
            accepted: to_write.len(),
            issues,
        })
    }

    pub(super) async fn list_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
    ) -> Result<Vec<KnowledgePropertyPo>> {
        self.memory_dao
            .list_active_properties(ctx, target_type, target_id)
            .await
    }

    pub(super) async fn list_entity_properties_batch(
        &self,
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)],
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>> {
        if targets.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        self.memory_dao
            .list_active_properties_batch(ctx, targets)
            .await
    }

    pub(super) async fn query_entities_by_properties(
        &self,
        ctx: RequestContext,
        filters: &[PropertyFilter],
    ) -> Result<Vec<String>> {
        if filters.is_empty() {
            return Ok(Vec::new());
        }

        let mut intersection: Option<HashSet<String>> = None;
        for filter in filters {
            // 运算符与值类型不匹配（如文本用 Gt）是调用方写错，直接报错：
            // DAO 侧会静默返回空集，但在这里静默放宽会让调用方误以为「筛过了」
            if !filter.is_supported() {
                bail_err!(
                    InvalidRequest,
                    "属性 {} 的反查运算符不支持值类型 {}（仅 number 支持比较运算）",
                    filter.term_key,
                    filter.value.data_type().as_str()
                );
            }
            let hits = self
                .memory_dao
                .query_properties_by_value(ctx.clone(), filter)
                .await?;
            let ids: HashSet<String> = hits.into_iter().map(|p| p.target_id).collect();
            intersection = Some(match intersection {
                None => ids,
                Some(acc) => acc.intersection(&ids).cloned().collect(),
            });
            // 已经空了就不用再查后面的条件
            if intersection.as_ref().is_some_and(|s| s.is_empty()) {
                break;
            }
        }

        let mut ids: Vec<String> = intersection.unwrap_or_default().into_iter().collect();
        ids.sort();
        Ok(ids)
    }
}
