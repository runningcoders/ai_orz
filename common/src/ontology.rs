//! 本体词表惰性解析（前后端共享的纯函数层）
//!
//! ## 分层
//!
//! 本模块是**纯计算层**：词表查找结构 + 漂移判定，无 IO、无时钟、无副作用。
//! 词表由后端 OntologyDal 全量载入构建（词表量级为个位数十词，全量载入成本
//! 可忽略）；前端复用同一套判定逻辑做图谱渲染侧的着色提示。
//!
//! ## 为什么漂移是纯函数而不是落库状态位
//!
//! 漂移 = "以当前本体词表为参数的计算结论"，不物化：
//! 某词今天在词表外（漂移），管理员明天把它收编为新关系词或加同义映射，
//! 下一次解析时历史漂移数据自动重新解释为规范/同义 —— 无需回填任何数据。
//! 解析结论因此不入图谱、不入 SQLite，只在消费侧转为统计事件（DuckDB）。
//!
//! ## 归一化约定
//!
//! `raw_term` 统一做 `trim + ASCII 小写`（[`normalize`]）。这是解析两侧
//! （词表构建 / 词条解析）唯一共享的规范形：词表 `term_key` 约定小写
//! snake_case，因此归一化 key 与词表规范形一致；带空格、大小写混写的
//! 原文（如 `"  Contains "`）都能命中规范词。刻意**不去下划线**——
//! 枚举解析（`normalize_enum_key`）需要跨书写形归并，而本体解析要求
//! `contained_by` 与 `containedby` 保持可区分，过度归并会掩盖真实漂移。

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 词条种类（解析请求的目标词表）
///
/// JSON 序列化形态为小写 `"class"` / `"relation"`，与 [`TermKind::as_str`]
/// 及统计事件 kind 取值保持同一口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TermKind {
    /// 实体类（图谱节点类型）
    Class,
    /// 关系类型（图谱边类型）
    Relation,
}

impl TermKind {
    /// 统计打点与日志使用的稳定标识（对齐监控事件 kind 取值）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Relation => "relation",
        }
    }
}

impl std::str::FromStr for TermKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "class" => Ok(Self::Class),
            "relation" => Ok(Self::Relation),
            _ => Err(()),
        }
    }
}

/// 单个词条的解析结论
///
/// 三分支互斥完备：规范词命中 / 同义映射命中 / 词表外漂移。
/// `raw_term` 字段一律携带**归一化后的原文**（见 [`normalize`]），
/// 便于消费侧把 `Contains` / `contains` 等书写变体归并到同一计数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTerm {
    /// 规范词命中：原文本身就在目标词表里
    Canonical {
        /// 命中的规范词（词表语义锚点，snake_case）
        term_key: String,
    },
    /// 同义映射命中：原文经映射表收敛到规范词
    ViaSynonym {
        /// 映射目标的规范词（词表语义锚点）
        term_key: String,
        /// 归一化后的原文
        raw_term: String,
    },
    /// 词表外 = 漂移（软门禁：不拦截写入，仅标记待审与计数）
    Drift {
        /// 归一化后的原文
        raw_term: String,
    },
}

/// 词表查找结构（启动/查询时由词表全量构建）
///
/// 三个集合的 key 均为 [`normalize`] 后的规范形；构建方保证
/// `synonyms` 的 value 指向两类规范词之一的原文。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OntologyLexicon {
    /// 实体类规范词集合（归一化 key）
    pub class_keys: HashSet<String>,
    /// 关系类型规范词集合（归一化 key）
    pub relation_keys: HashSet<String>,
    /// 同义映射：(归一化 raw_term, 目标 kind) → 目标规范词原文
    ///
    /// 以 target_kind 做复合键：同一 raw_term 可分别映射到实体类与关系词，
    /// 解析时按请求 kind 精确命中，避免跨 kind 映射误判漂移。
    pub synonyms: HashMap<(String, TermKind), String>,
}

impl OntologyLexicon {
    /// 词表是否完全为空（三类均无词条）
    pub fn is_empty(&self) -> bool {
        self.class_keys.is_empty() && self.relation_keys.is_empty() && self.synonyms.is_empty()
    }
}

/// term_key / raw_term 的最大长度（字符数；与迁移 CHECK 约束、Domain 写侧校验对齐）
pub const MAX_TERM_KEY_LEN: usize = 256;

/// 归一化原文：trim + ASCII 小写
///
/// 解析两侧（词表构建 / 词条解析）唯一共享的规范形，见模块头约定。
/// pub 供 Domain/DAL 层复用：term_key 查重、预置词表比对等场景必须
/// 与本函数口径一致，禁止各处手写 `to_lowercase` 造成归并形分叉。
pub fn normalize(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// 解析入口：判定一个词条相对当前词表的三分支结论
///
/// 判定顺序（优先级从高到低）：
/// 1. 空词 → 漂移（空词必然不在任何词表，兜底不 panic）；
/// 2. 原文命中 `kind` 对应词集 → [`ResolvedTerm::Canonical`]；
/// 3. 同义映射命中，且映射目标属于 `kind` 对应词集 → [`ResolvedTerm::ViaSynonym`]
///    （目标属于另一 kind 视为该 kind 下无效映射，按漂移处理）；
/// 4. 其余 → [`ResolvedTerm::Drift`]。
///
/// 纯函数：同一输入 + 同一词表 ⇒ 同一结论。
pub fn resolve(lexicon: &OntologyLexicon, kind: TermKind, raw_term: &str) -> ResolvedTerm {
    let raw = normalize(raw_term);
    if raw.is_empty() {
        return ResolvedTerm::Drift { raw_term: raw };
    }
    let keys = match kind {
        TermKind::Class => &lexicon.class_keys,
        TermKind::Relation => &lexicon.relation_keys,
    };
    if keys.contains(&raw) {
        return ResolvedTerm::Canonical { term_key: raw };
    }
    if let Some(target) = lexicon.synonyms.get(&(raw.clone(), kind)) {
        let target = normalize(target);
        if keys.contains(&target) {
            return ResolvedTerm::ViaSynonym {
                term_key: target,
                raw_term: raw,
            };
        }
    }
    ResolvedTerm::Drift { raw_term: raw }
}

// ==================== 认证报告（certify 聚合） ====================

/// 单条词条的认证结论（kind + 解析分支）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedTerm {
    /// 词条种类
    pub kind: TermKind,
    /// 解析结论（三分支之一，见 [`resolve`]）
    pub verdict: ResolvedTerm,
}

/// 一批词条的认证报告（certify 聚合结果）
///
/// 消费侧逐条调用 [`OntologyCertifyReport::record`] 聚合，保持输入顺序；
/// 计数与清单方法供返回值、日志与测试断言复用。纯数据结构，不携带
/// 任何落库状态——置位 / 打点动作由消费侧依据本报告执行（软门禁）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OntologyCertifyReport {
    /// 逐条结论（输入顺序）
    pub entries: Vec<CertifiedTerm>,
}

impl OntologyCertifyReport {
    /// 聚合一条解析结论
    pub fn record(&mut self, kind: TermKind, verdict: ResolvedTerm) {
        self.entries.push(CertifiedTerm { kind, verdict });
    }

    /// 规范词命中条数（Canonical 分支）
    pub fn canonical_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.verdict, ResolvedTerm::Canonical { .. }))
            .count()
    }

    /// 同义映射命中条数（ViaSynonym 分支）
    pub fn via_synonym_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.verdict, ResolvedTerm::ViaSynonym { .. }))
            .count()
    }

    /// 漂移条数（Drift 分支）
    pub fn drift_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.verdict, ResolvedTerm::Drift { .. }))
            .count()
    }

    /// 认证词条总数
    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    /// 认证通过的规范词清单（Canonical ∪ ViaSynonym 的 term_key，去重保序）
    ///
    /// 置位语义的消费键：节点词条规范词命中即 `is_published` 置位。
    pub fn certified_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for entry in &self.entries {
            let key = match &entry.verdict {
                ResolvedTerm::Canonical { term_key } => term_key,
                ResolvedTerm::ViaSynonym { term_key, .. } => term_key,
                ResolvedTerm::Drift { .. } => continue,
            };
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        keys
    }

    /// 漂移原文清单（归一化形，去重保序）
    ///
    /// 打点语义的消费键：每个漂移词对应一条 OntologyDriftEvent 事件。
    pub fn drift_terms(&self) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        for entry in &self.entries {
            if let ResolvedTerm::Drift { raw_term } = &entry.verdict
                && !terms.contains(raw_term)
            {
                terms.push(raw_term.clone());
            }
        }
        terms
    }

    /// 是否零漂移（全部词条均在词表内命中）
    pub fn is_clean(&self) -> bool {
        self.drift_count() == 0
    }
}

// ==================== 预置词表注入契约（seed 同步） ====================

/// 预置实体类条目（字段对齐 `CreateOntologyClassRequest`）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresetOntologyClass {
    /// 规范词 key（小写 snake_case）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 实体必填属性清单
    #[serde(default)]
    pub required_fields: Vec<String>,
}

/// 预置关系类型条目（字段对齐 `CreateOntologyRelationTypeRequest`）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresetOntologyRelationType {
    /// 规范词 key（小写 snake_case）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 允许的源实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub domain_classes: Vec<String>,
    /// 允许的目标实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub range_classes: Vec<String>,
    /// 边权重基数（None = 默认 1.0）
    #[serde(default)]
    pub weight_base: Option<f64>,
    /// 反向关系 key（None = 无对称反向词）
    #[serde(default)]
    pub inverse_key: Option<String>,
}

/// 预置同义映射条目（字段对齐 `CreateOntologySynonymRequest`）
///
/// 不 derive `Default`：`target_kind` 为必填枚举（[`TermKind`] 无默认形），
/// seed 数据三字段必然显式给出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresetOntologySynonym {
    /// 漂移原文（入库前归一化：trim + 小写）
    pub raw_term: String,
    /// 目标词条种类
    pub target_kind: TermKind,
    /// 目标规范词 key
    pub target_key: String,
}

/// 预置本体词表（seed 快照 ontology 段的统一形态）
///
/// 三段结构与词表三张表一一对应；注入流程（apply_preset_ontology）
/// 按"仅补缺"策略逐条比对 term_key / raw_term，已存在条目跳过——
/// 本体是共享约定，seed 不覆盖管理页的本地修改。比对前两侧统一走
/// [`normalize`] 归一，避免书写形差异导致重复插入。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresetOntologyLexicon {
    /// 实体类条目
    #[serde(default)]
    pub classes: Vec<PresetOntologyClass>,
    /// 关系类型条目
    #[serde(default)]
    pub relation_types: Vec<PresetOntologyRelationType>,
    /// 同义映射条目
    #[serde(default)]
    pub synonym_mappings: Vec<PresetOntologySynonym>,
}

impl PresetOntologyLexicon {
    /// 三段条目总数（预览响应 total 口径）
    pub fn total_count(&self) -> usize {
        self.classes.len() + self.relation_types.len() + self.synonym_mappings.len()
    }

    /// 词表是否为空（三段均无条目）
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

// ==================== 词表视图 + 注入报告（Domain 契约） ====================

/// 词表条目摘要（实体类 / 关系类型通用行形态：规范词 + 展示名 + 语义描述）
///
/// [`OntologyLexiconSummary`] 的行形态：提示词注入只给 term_key 无法引导
/// 模型正确用词，需要展示名与语义描述；字段刻意收窄到三项控制注入体积。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LexiconTermSummary {
    /// 规范词 key（snake_case）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
}

/// 词表同义映射样例行
///
/// 不 derive `Default`：`target_kind` 为必填枚举（[`TermKind`] 无默认形）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LexiconSynonymSummary {
    /// 原始词（归一化形）
    pub raw_term: String,
    /// 目标词条种类
    pub target_kind: TermKind,
    /// 目标规范词 key
    pub target_key: String,
}

/// 词表注入视图（`OntologyDomain::list_lexicon` 返回）
///
/// 消费方是神经技能提示词构建器：Token 预算超限时按"关系词 > 实体类 >
/// 同义样例"顺序裁剪（design §3），因此三段分列且字段顺序即裁剪优先级。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OntologyLexiconSummary {
    /// 关系类型词条（注入优先级最高）
    pub relation_types: Vec<LexiconTermSummary>,
    /// 实体类词条
    pub classes: Vec<LexiconTermSummary>,
    /// 同义映射样例
    pub synonyms: Vec<LexiconSynonymSummary>,
}

impl OntologyLexiconSummary {
    /// 三段条目总数
    pub fn total_count(&self) -> usize {
        self.relation_types.len() + self.classes.len() + self.synonyms.len()
    }

    /// 词表为空（三段均无条目——提示词注入跳过，看板引导初始注入）
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

/// 预置词表注入报告（"仅补缺"策略的执行结果）
///
/// `inserted_*` 清单供管理页同步响应展示"本次新增了什么"；`skipped`
/// 是已存在条目数（三段合计），幂等注入的证明。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OntologyLexiconApplyReport {
    /// 新增实体类 term_key 清单（按注入顺序）
    pub inserted_classes: Vec<String>,
    /// 新增关系类型 term_key 清单（按注入顺序）
    pub inserted_relation_types: Vec<String>,
    /// 新增同义映射条数
    pub inserted_synonyms: usize,
    /// 跳过条目数（term_key / raw_term 已存在，三段合计）
    pub skipped: usize,
}

/// 词表缺口预览报告（seed 手动同步 preview 用，只读对比不写库）
///
/// 判定与 [`OntologyLexiconApplyReport`] 的注入逻辑同源（term_key / raw_term
/// 归一化后物理存在即算已存在，含退役行），保证 preview "将新增" 与 sync
/// 实际结果一致。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LexiconGapReport {
    /// 缺失实体类 term_key 清单（归一化，将新增）
    pub missing_classes: Vec<String>,
    /// 缺失关系类型 term_key 清单（归一化，将新增）
    pub missing_relation_types: Vec<String>,
    /// 缺失同义映射条数（仅计数不列明细：raw→target 映射无显示名，前端以「N 条」呈现）
    pub missing_synonyms: usize,
    /// 已存在条目数（三段合计，注入时将跳过）
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自包含测试词表：不与枚举词表耦合，专注判定逻辑本身
    fn fixture_lexicon() -> OntologyLexicon {
        OntologyLexicon {
            class_keys: ["agent", "task", "document"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            relation_keys: ["contains", "depends", "similar"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            synonyms: [
                (
                    ("包含".to_string(), TermKind::Relation),
                    "contains".to_string(),
                ),
                (
                    ("依赖".to_string(), TermKind::Relation),
                    "depends".to_string(),
                ),
                (
                    ("object".to_string(), TermKind::Class),
                    "document".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn canonical_hit_on_relation_vocabulary() {
        let lexicon = fixture_lexicon();
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "contains"),
            ResolvedTerm::Canonical {
                term_key: "contains".to_string()
            }
        );
    }

    #[test]
    fn canonical_hit_ignores_case_and_surrounding_whitespace() {
        let lexicon = fixture_lexicon();
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "  Contains "),
            ResolvedTerm::Canonical {
                term_key: "contains".to_string()
            }
        );
    }

    #[test]
    fn via_synonym_hit_maps_to_target() {
        let lexicon = fixture_lexicon();
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "包含"),
            ResolvedTerm::ViaSynonym {
                term_key: "contains".to_string(),
                raw_term: "包含".to_string()
            }
        );
    }

    #[test]
    fn via_synonym_target_must_belong_to_requested_kind() {
        let lexicon = fixture_lexicon();
        // "object" 映射到实体类 document；按关系词解析时应判漂移
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "object"),
            ResolvedTerm::Drift {
                raw_term: "object".to_string()
            }
        );
        // 同一词条按实体类解析则同义命中
        assert_eq!(
            resolve(&lexicon, TermKind::Class, "object"),
            ResolvedTerm::ViaSynonym {
                term_key: "document".to_string(),
                raw_term: "object".to_string()
            }
        );
    }

    #[test]
    fn out_of_vocabulary_is_drift() {
        let lexicon = fixture_lexicon();
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "买卖"),
            ResolvedTerm::Drift {
                raw_term: "买卖".to_string()
            }
        );
    }

    #[test]
    fn kind_selects_which_vocabulary_is_checked() {
        let lexicon = fixture_lexicon();
        // "agent" 是实体类；按关系词解析应判漂移
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "agent"),
            ResolvedTerm::Drift {
                raw_term: "agent".to_string()
            }
        );
        assert_eq!(
            resolve(&lexicon, TermKind::Class, "agent"),
            ResolvedTerm::Canonical {
                term_key: "agent".to_string()
            }
        );
    }

    #[test]
    fn canonical_takes_priority_over_synonym_entry() {
        let lexicon = fixture_lexicon();
        // 原文本身是规范词，即使同义映射里也有同名条目也不应绕道
        let mut lexicon = lexicon;
        lexicon.synonyms.insert(
            ("contains".to_string(), TermKind::Relation),
            "similar".to_string(),
        );
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "contains"),
            ResolvedTerm::Canonical {
                term_key: "contains".to_string()
            }
        );
    }

    #[test]
    fn empty_raw_term_is_drift_not_panic() {
        let lexicon = fixture_lexicon();
        assert_eq!(
            resolve(&lexicon, TermKind::Relation, "   "),
            ResolvedTerm::Drift {
                raw_term: String::new()
            }
        );
    }

    #[test]
    fn same_input_and_lexicon_give_same_verdict() {
        let lexicon = fixture_lexicon();
        let first = resolve(&lexicon, TermKind::Relation, "  DEPENDS ");
        let second = resolve(&lexicon, TermKind::Relation, "depends");
        assert_eq!(first, second);
    }

    #[test]
    fn term_kind_roundtrips_through_str() {
        assert_eq!(TermKind::Class.as_str(), "class");
        assert_eq!(TermKind::Relation.as_str(), "relation");
        assert_eq!("class".parse::<TermKind>(), Ok(TermKind::Class));
        assert_eq!("relation".parse::<TermKind>(), Ok(TermKind::Relation));
        assert!("edge".parse::<TermKind>().is_err());
    }

    #[test]
    fn empty_lexicon_reports_everything_as_drift() {
        let lexicon = OntologyLexicon::default();
        assert!(lexicon.is_empty());
        assert_eq!(
            resolve(&lexicon, TermKind::Class, "agent"),
            ResolvedTerm::Drift {
                raw_term: "agent".to_string()
            }
        );
    }

    #[test]
    fn normalize_is_shared_convention() {
        assert_eq!(normalize("  Contains "), "contains");
        assert_eq!(normalize("AGENT"), "agent");
        // 刻意不去下划线：contained_by 与 containedby 必须保持可区分
        assert_eq!(normalize("contained_by"), "contained_by");
        assert_ne!(normalize("contained_by"), normalize("containedby"));
    }

    #[test]
    fn certify_report_aggregates_counts_and_keys() {
        let lexicon = fixture_lexicon();
        let mut report = OntologyCertifyReport::default();
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "contains"),
        );
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "包含"),
        );
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "买卖"),
        );
        report.record(
            TermKind::Class,
            resolve(&lexicon, TermKind::Class, "document"),
        );
        assert_eq!(report.canonical_count(), 2);
        assert_eq!(report.via_synonym_count(), 1);
        assert_eq!(report.drift_count(), 1);
        assert_eq!(report.total_count(), 4);
        assert_eq!(report.certified_keys(), ["contains", "document"]);
        assert_eq!(report.drift_terms(), ["买卖"]);
        assert!(!report.is_clean());
    }

    #[test]
    fn certify_report_dedups_keys_and_keeps_order() {
        let lexicon = fixture_lexicon();
        let mut report = OntologyCertifyReport::default();
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "contains"),
        );
        // 同一规范词的书写变体（归一化后同形）不应产生重复 key
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "  Contains "),
        );
        report.record(
            TermKind::Relation,
            resolve(&lexicon, TermKind::Relation, "depends"),
        );
        assert_eq!(report.certified_keys(), ["contains", "depends"]);
        assert!(report.drift_terms().is_empty());
        assert!(report.is_clean());
    }

    #[test]
    fn preset_lexicon_deserializes_with_defaults() {
        let json = r#"{
            "classes": [
                {"term_key": "agent", "display_name": "智能体", "description": "自主执行单元"}
            ],
            "relation_types": [
                {"term_key": "contains", "display_name": "包含", "description": "组成关系"}
            ],
            "synonym_mappings": [
                {"raw_term": "包含", "target_kind": "relation", "target_key": "contains"}
            ]
        }"#;
        let preset: PresetOntologyLexicon = serde_json::from_str(json).expect("deserialize");
        assert_eq!(preset.classes.len(), 1);
        assert!(preset.classes[0].required_fields.is_empty());
        assert_eq!(preset.relation_types.len(), 1);
        assert_eq!(preset.relation_types[0].weight_base, None);
        assert_eq!(preset.relation_types[0].inverse_key, None);
        assert_eq!(preset.synonym_mappings.len(), 1);
        assert_eq!(preset.synonym_mappings[0].target_kind, TermKind::Relation);
        assert_eq!(preset.total_count(), 3);
        assert!(!preset.is_empty());
    }

    #[test]
    fn empty_preset_lexicon_roundtrips() {
        let preset = PresetOntologyLexicon::default();
        assert!(preset.is_empty());
        assert_eq!(preset.total_count(), 0);
        let json = serde_json::to_string(&preset).expect("serialize");
        let back: PresetOntologyLexicon = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, preset);
    }
}
