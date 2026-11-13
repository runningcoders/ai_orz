//! Memory 记忆系统模型
//!
//! 定义记忆系统相关的实体类型：
//! - MemoryTrace - 记忆追踪条目，一条原始记忆，包含完整信息，ID = 内容 hash
//! - ShortTermMemoryIndexPo - 短期记忆索引（SQLite 持久化）
//! - LongTermKnowledgeNodePo - 长期知识图谱节点（SQLite 持久化）
//! - KnowledgeReferencePo - 知识节点引用原始短期索引
//! - Memory - 记忆业务实体（包含 PO + 搜索匹配信息）

use crate::models::vector::{MatchType, SearchMatchInfo, VectorPayload, Vectorizable};
use common::api::{MemoryResult, MemorySearchMatch};
use common::enums::ontology::PropertyTargetType;
use common::enums::{KnowledgeRelationStatus, KnowledgeRelationType, MemoryStatus};
use common::ontology_property::{PropertyColumns, PropertyValue};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

/// 记忆追踪条目
///
/// 一条原始记忆，对应一次完整的思考闭环（输入 → 模型思考 → 输出）
/// 所有字段都使用统一的 trace_id 贯穿，模型可以看到 trace_id 并自引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTrace {
    /// 唯一 ID = trace-{agent_id}-{timestamp}
    pub id: String,
    /// 所属 Agent ID
    pub agent_id: String,
    /// 所属任务 ID（可选，用于追溯到具体任务）
    pub task_id: Option<String>,
    /// 请求日志 ID（来源溯源）
    pub log_id: String,
    /// 创建者用户 ID（来源溯源）
    pub user_id: String,
    /// 所属组织 ID（来源溯源）
    pub organization_id: String,
    /// 角色
    pub role: common::enums::MemoryRole,

    // ========== 思考闭环字段 ==========
    /// 思考输入（完整 Prompt）
    pub input: String,
    /// 思考输出（模型返回，可能为空表示中断）
    pub output: Option<String>,
    /// 思考创建时间（输入时间）
    pub created_at: i64,
    /// 思考完成时间（输出写入时间）
    pub completed_at: Option<i64>,

    /// 元数据（可扩展存储额外信息）
    pub metadata: HashMap<String, String>,
    /// 物理位置（DAO 写入或查询后回填，不参与序列化）
    #[serde(skip)]
    pub position: Option<MemoryTracePosition>,
}

/// MemoryTrace 在每日 JSONL 文件中的物理位置
///
/// DAO 层 `append_trace` / `batch_append_traces` 写入后返回，
/// DAL 层可用于：
/// - 构造 `KnowledgeReferencePo`（date_path + line_number 定位原始内容）
/// - 后续创建 `ShortTermMemoryIndexPo` 时关联 trace_ids
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryTracePosition {
    /// trace 的内容 hash ID
    pub trace_id: String,
    /// 日期文件名，如 "20260512.jsonl"
    pub date_filename: String,
    /// 行号（0-indexed）
    pub line_number: u64,
}

impl MemoryTrace {
    /// 创建新的 MemoryTrace（思考闭环）
    ///
    /// 自动生成 trace_id = trace-{agent_id}-{timestamp}
    pub fn new(
        agent_id: String,
        log_id: String,
        user_id: String,
        organization_id: String,
        role: common::enums::MemoryRole,
        input: String,
        task_id: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        let created_at = now.timestamp();
        // 加随机后缀避免同一 agent 并发处理两条消息时 timestamp_nanos 相同导致 trace_id 碰撞
        let trace_id = format!(
            "trace-{}-{}-{}",
            agent_id,
            now.timestamp_nanos_opt().unwrap_or(0),
            rand::random::<u16>()
        );
        Self {
            id: trace_id,
            agent_id,
            task_id,
            log_id,
            user_id,
            organization_id,
            role,
            input,
            output: None,
            created_at,
            completed_at: None,
            metadata: HashMap::new(),
            position: None,
        }
    }

    /// 完成思考，回填输出
    pub fn complete(&mut self, output: String) {
        self.output = Some(output);
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// 格式化为 markdown 写入每日文件
    pub fn to_markdown(&self) -> String {
        let role = match &self.role {
            common::enums::MemoryRole::System => "**System**",
            common::enums::MemoryRole::User => "**User**",
            common::enums::MemoryRole::Assistant => "**Assistant**",
            common::enums::MemoryRole::Summary => "**Summary**",
        };

        let mut content = format!(
            r#"
---
ID: {}
Role: {}
Created: {}
"#,
            self.id, role, self.created_at,
        )
        .trim()
        .to_string();

        // 写入 Input
        content.push_str(&format!("\n\n### Input\n\n{}\n", self.input));

        // 如果有 Output，也写入
        if let Some(output) = &self.output {
            content.push_str(&format!("\n### Output\n\n{}\n", output));
        }

        // 如果有完成时间，写入
        if let Some(completed_at) = self.completed_at {
            content.push_str(&format!("\nCompleted: {}\n", completed_at));
        }

        content + "\n\n"
    }
}

/// 短期记忆索引 PO
///
/// 每条短期记忆聚合了多条相关记忆细节，存储在 SQLite
/// 通过 trace_ids 显式记录聚合的 trace id 列表
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ShortTermMemoryIndexPo {
    /// 唯一 ID = 多个原始记忆细节 id 拼接后二次 hash
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 所属任务 ID（可选，用于追溯到具体任务）
    pub task_id: Option<String>,
    /// 角色
    pub role: String,
    /// 归纳摘要（用于全文检索）
    pub summary: String,
    /// 标签列表（用于过滤检索，JSON 数组字符串）
    pub tags: String,
    /// 聚合的 trace id 列表（JSON 数组字符串）
    pub trace_ids: String,
    /// 记忆状态
    pub status: common::enums::MemoryStatus,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

impl ShortTermMemoryIndexPo {
    /// 构建用于向量索引的文本（summary + tags 拼接）
    ///
    /// tags 为 JSON 数组字符串，会展平为空格分隔的纯文本；
    /// 空标签或解析失败时仅返回 summary
    fn vector_text(&self) -> String {
        let tags = flatten_tags(&self.tags);
        if tags.is_empty() {
            self.summary.clone()
        } else {
            format!("{}\n{}", self.summary, tags)
        }
    }
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for ShortTermMemoryIndexPo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:short_term"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            task_id: self.task_id.clone(),
            status: Some(self.status.to_i32().to_string()),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
}

/// 长期知识图谱节点 PO
///
/// 经过归纳总结得到的知识节点，存储在 SQLite
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LongTermKnowledgeNodePo {
    /// 唯一 ID
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 节点名称
    pub node_name: String,
    /// 节点描述
    pub node_description: String,
    /// 节点类型：concept / event / preference / skill / ...
    pub node_type: String,
    /// 综合总结
    pub summary: String,
    /// 标签列表（用于过滤检索 + 全文索引，JSON 数组字符串）
    ///
    /// ⚠️ **过渡期保留、不再是权威读写面**：写侧与 [`Self::user_tags`] 双写
    /// （FTS5 触发器 `knowledge_node_fts_ai/_au` 与短词 LIKE 兜底都读本列），
    /// 读侧仅作回退兼容。删除留待后续单。
    pub tags: String,
    /// 用户 / Agent **可写业务标签**（JSON 数组字符串）
    ///
    /// **唯一权威读写面**：`tags` / `node_tags` 参数一律落本列。
    /// 不含 `published` 这类系统标记 —— 那些由 [`Self::is_published`] 承载，
    /// 回显时派生到 [`MemoryResult::system_flags`]。
    pub user_tags: String,
    /// 记忆状态
    pub status: common::enums::MemoryStatus,
    /// 是否被标记为**高价值/高影响力**（tags 含 "published" 时为 true）
    ///
    /// 冗余字段，与 tags 中的 "published" 标签同步，用于走索引排序。
    /// ⚠️ 它**不是可见性控制位**：知识节点在蜂巢内对所有 Agent 可见，与是否 published 无关。
    pub is_published: bool,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

impl LongTermKnowledgeNodePo {
    /// 构建用于向量索引的文本（node_name + node_description + summary + tags 拼接）
    ///
    /// node_name 是节点最核心的语义锚点（如「Rust 异步运行时」），必须参与 embedding，
    /// 否则按名称/主题搜节点时向量侧几乎无法命中；
    /// tags 为 JSON 数组字符串，会展平为空格分隔的纯文本；
    /// 空标签或解析失败时仅返回 node_name + node_description + summary
    fn vector_text(&self) -> String {
        let tags = flatten_tags(&self.tags);
        if tags.is_empty() {
            format!(
                "{}\n{}\n{}",
                self.node_name, self.node_description, self.summary
            )
        } else {
            format!(
                "{}\n{}\n{}\n{}",
                self.node_name, self.node_description, self.summary, tags
            )
        }
    }
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for LongTermKnowledgeNodePo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:knowledge_node"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            entity_type: Some(self.node_type.clone()),
            status: Some(self.status.to_i32().to_string()),
            is_published: Some(self.is_published),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
}

/// 知识节点关系 PO
///
/// 专门存储知识节点之间的关系，独立表方便查询和维护
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeNodeRelationPo {
    /// 唯一 ID
    pub id: String,
    /// 源节点 ID
    pub source_node_id: String,
    /// 目标节点 ID
    pub target_node_id: String,
    /// 关系类型**原文** —— 写入方标注的字符串，落库不做任何归一化
    ///
    /// ⚠️ 这里存 `String` 而不是枚举：枚举只有十几个变体，词表外的标注一旦
    /// 走 `KnowledgeRelationType::from()` 就会被塌成 `Custom`，**原文永久丢失**
    /// （图谱上只能显示「自定义」，Agent 明明标了具体的语义却看不出来）。
    /// 中文/美化映射只发生在展示期，见 `KnowledgeRelationType::zh_label_from_display`。
    pub relation_type: String,
    /// 关系强度（0.0~1.0，越大越强），`None` = 未标注
    ///
    /// 由写入方（`save_long_term_memory` 的 `relations[].weight`）声明，
    /// 图谱按它调线宽与浓淡；`None` 渲染基准线宽。
    /// ⚠️ 与「强度 0」不是一回事：0 是「明确很弱」，`None` 是「没人标过」。
    pub weight: Option<f32>,
    /// 边状态（生效/已替换）
    ///
    /// 只表达**边自身**的版本生命周期：修正关系 = 插入新边 + 旧边降级
    /// `Superseded`，历史边永久留库（因果链回放可还原「当时怎么连的」）。
    /// ⚠️ 与两端节点的 `MemoryStatus` 刻意不对等：节点遗忘不改边状态，
    /// 该联动在读侧派生（端点不入批 → 边随批次丢弃；节点恢复后边自然回归）。
    /// 默认查询只取 `Active`；同键 (source, target, relation_type) 仅一条生效
    /// （部分唯一索引 `uq_knowledge_relation_active_edge` 保证）。
    pub status: KnowledgeRelationStatus,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 知识节点属性 PO（ABox：属性值，版本化）
///
/// 补齐 ABox 缺失的属性落点：此前属性只能用 `attribute_of` / `value_of`
/// 两条关系词「用边模拟属性」，值不可索引、无数据类型、无值域，
/// 也没法回答「查所有 model=xxx 的 agent 节点」这类结构化过滤。
///
/// **EAV 行存储**：每行一个属性值，四个类型化值列（`value_text` /
/// `value_num` / `value_bool` / `value_ref`）由 `data_type` 指派其一，
/// 未使用的列落 NULL。这样数值属性可直接 `WHERE value_num > ?` 走索引，
/// 引用属性可直接 JOIN 回节点表——单列 JSON 做不到这两点。
///
/// **图谱域内通用**：一张表同时承载节点属性与边属性，靠 `target_type`
/// 解释 `target_id` 指向哪张表（见迁移决策 #6）。**不跨域**扩展：
/// agent / task / project 带 `organization_id`，而图谱与本体表一律不带，
/// 混进一张表会撕裂作用域约定。
///
/// **版本化**：修正 = 旧行降级 `Superseded` + 插入新行，历史全留，
/// 可回答「当时这个值是多少」；部分唯一索引 `uq_knowledge_property_active`
/// 保证同一 (target_type, target_id, term_key, ordinal) 只有一条生效。
/// 与知识边表的版本化先例同构。
///
/// 属性定义（类型 / 值域 / 基数 / 适用载体）在 TBox `ontology_properties`；
/// 值的形态与校验纯函数见 `common::ontology_property`。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgePropertyPo {
    /// 唯一 ID
    pub id: String,
    /// 挂载载体类型：`node` → `long_term_knowledge_node.id` /
    /// `relation` → `knowledge_node_relation.id`
    pub target_type: String,
    /// 载体 ID（裸列无外键，归属由 `target_type` 解释——ABox 既有惯例）
    pub target_id: String,
    /// 属性语义锚点，指向 `ontology_properties.term_key`（非 id）
    pub term_key: String,
    /// 值类型（冗余落库：属性定义退役后存量值仍可解释）
    pub data_type: String,
    /// 多值槽位序号：`single` 型恒 0，`many` 型从 0 递增
    pub ordinal: i32,
    /// 文本列（`string` / `date` / `enum` 型承载）
    pub value_text: Option<String>,
    /// 数值列（`number` 型承载；量化指标的过滤入口）
    pub value_num: Option<f64>,
    /// 布尔列（`boolean` 型承载；false 落 0，未赋值落 NULL）
    pub value_bool: Option<i32>,
    /// 引用列（`reference` 型承载；指向另一个节点 id 或外部实体标识）
    pub value_ref: Option<String>,
    /// 行版本生命周期：1 生效 / 0 被取代 / 2 已删除（与边状态同口径）
    pub status: KnowledgeRelationStatus,
    /// 创建时间戳（秒级）
    pub created_at: i64,
    /// 更新时间戳（秒级）
    pub updated_at: i64,
}

impl KnowledgePropertyPo {
    /// 创建一条生效属性值（id / status / 时间戳自动填充）
    ///
    /// 值按 [`PropertyValue::into_columns`] 拆到对应列；`ordinal` 由调用方
    /// 给出（`single` 型传 0，`many` 型按槽位递增）。
    ///
    /// 载体用枚举 [`PropertyTargetType`] 而不是裸字符串传入：泛化表的最大风险
    /// 是 `target_type` 拼错导致静默查不到（无外键兜底），构造入口收窄类型
    /// 可把这个错误提前到编译期。
    pub fn new(
        target_type: PropertyTargetType,
        target_id: impl Into<String>,
        term_key: impl Into<String>,
        value: PropertyValue,
        ordinal: i32,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        let data_type = value.data_type().as_str().to_string();
        let cols = value.into_columns();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            target_type: target_type.as_str().to_string(),
            target_id: target_id.into(),
            term_key: term_key.into(),
            data_type,
            ordinal,
            value_text: cols.value_text,
            value_num: cols.value_num,
            value_bool: cols.value_bool,
            value_ref: cols.value_ref,
            status: KnowledgeRelationStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析挂载载体；脏数据兜底退 `node`（软门禁，不 panic 不阻断）
    pub fn target(&self) -> PropertyTargetType {
        PropertyTargetType::parse_or_default(&self.target_type)
    }

    /// 解析值类型；脏数据兜底退 `string`（软门禁，不 panic 不阻断）
    pub fn data_type(&self) -> common::enums::ontology::PropertyDataType {
        common::enums::ontology::PropertyDataType::parse_or_default(&self.data_type)
    }

    /// 按声明类型还原属性值（指派列缺失返 `None` = 脏数据行）
    pub fn value(&self) -> Option<PropertyValue> {
        PropertyValue::from_columns(
            self.data_type(),
            PropertyColumns {
                value_text: self.value_text.clone(),
                value_num: self.value_num,
                value_bool: self.value_bool,
                value_ref: self.value_ref.clone(),
            },
        )
    }

    /// 是否生效（参与读路径）
    pub fn is_active(&self) -> bool {
        matches!(self.status, KnowledgeRelationStatus::Active)
    }
}

/// 知识节点引用原始记忆细节
///
/// 记录知识节点引用了哪些原始记忆细节，同时存储原始细节位置信息
/// 每条原始记忆细节单独一条引用记录，位置信息完整可追溯
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct KnowledgeReferencePo {
    /// 唯一 ID
    pub id: String,
    /// 知识节点 ID
    pub knowledge_id: String,
    /// 短期记忆索引 ID（这条原始细节属于哪个短期记忆索引）
    pub short_term_id: String,
    /// 原始记忆细节 ID（MemoryTrace.id）
    pub trace_id: String,
    /// 日期文件名：YYYYMMDD.jsonl，存储在 agent 目录下
    pub date_path: String,
    /// 在 JSONL 文件中的行号（0-based）
    pub line_number: i64, // SQLite 不支持 u64 直接存储，用 i64 足够
    /// 创建时间戳
    pub created_at: i64,
}

/// 记忆底层 PO 统一枚举
#[derive(Debug, Clone)]
pub enum MemoryPo {
    /// 原始记忆追踪（来自 JSONL，无 SQLite 表）
    Trace(MemoryTrace),
    /// 短期记忆索引
    ShortTerm(ShortTermMemoryIndexPo),
    /// 长期知识节点
    KnowledgeNode(LongTermKnowledgeNodePo),
    /// 知识节点关系
    Relation(KnowledgeNodeRelationPo),
}

// ==================== 记忆索引分层 ====================
//
// 【历史对话】只做「阶梯式脑容量加载」，不再按天数分档：
//
//   预算 = 上下文压缩阈值 × 10%（与 `context_overflow_threshold` 同源取值）
//   四档配额 = 预算 × 4:3:2:1（全文 / 3 段 / 2 段 / 1 段，由近到远）
//   从最新往最旧逐行渲染、累加估算 token；某档配额用满就切到下一档，
//   最后一档也用满即停止 —— 保留多少条完全由字符预算决定，**不设条数上限**。
//
// 时间**不参与分档**，只做三件参考性的事：
//   1. T0（入站消息 created_at）作锚点，created_at > T0 的新记忆不进块，
//      保证同一次唤醒链内 Phase 1 / Phase 2 的 base 逐字节一致；
//   2. 元信息给出可读绝对时间 + Unix 秒级时间戳（`ts=`），后者是**机器可读**
//      的过滤把手（模型可直接按数值比较时间窗，不必反推格式化日期）；
//   3. 元信息里额外标注相对时间（当天 / 昨天 / N天前 / N个月前），让模型
//      不必自己做日期减法就知道这条记忆有多老。
//
// ⚠️ 四条不变量：
// 1. **层级由上层算好传入**：PO 方法不读时钟、不查库、不依赖全局配置，
//    只做「元信息 + 摘要」的纯打包；T0 与预算都是入参。
// 2. **元信息不分级**：id / 绝对时间 / ts / 相对时间 / tags / task_id 是最基础的信息，
//    四档逐字节相同；只有摘要正文随档位衰减。
// 3. **相对时间以 T0 为基准，不用 `now()`**：同一次唤醒链内固定，且只用离散
//    天数桶，只有跨天（质变）才变，不做分钟级漂移。
//    ⚠️ 这有意放宽了 wiki 卡 §4-16「元信息只用绝对时间」的红线：跨唤醒时
//    同一条记忆的相对标签会变，索引块前缀会失配；换取的是模型无需心算日期。
//    `ts` 本身是绝对事实、跨唤醒不变，可作为稳定过滤条件。
// 4. **不设条数上限**：分页按预算**按需翻页**（不够才查下一页），
//    保留条数只由预算裁决；页数上限只防失控扫描，不是保留条数上限。
//
// 这些常量目前是静态的；未来若需按 Agent / 场景动态化，只需替换预算来源，
// 调用侧「层级 + 系数 + 锚点」三个入参保持不变。

/// 分页向前查询：每页条数
pub const MEMORY_INDEX_PAGE_SIZE: usize = 20;

/// 防失控扫描的页数硬上限（20 × 50 = 1000 行候选）
///
/// ⚠️ **不是保留条数上限**：正常情况下在预算装满、或 DB 翻到头（页长不足）
/// 时就会停；只有「整页都是空摘要/未来记忆，永远不消耗预算」这类病态数据
/// 才会顶到这里。留着它是为了避免一个超大 Agent 被打成几千次分页查询。
pub const MEMORY_INDEX_MAX_PAGES: usize = 50;

/// 一天的秒数（短期记忆 `created_at` 是秒级时间戳）
pub const MEMORY_INDEX_DAY_SECS: i64 = 86_400;

// ---- 脑容量：压缩阈值 → token 预算 → 四档配额 ----

/// 索引块占用的「脑容量」份额（%）：上下文压缩阈值的十分之一
pub const MEMORY_INDEX_CONTEXT_SHARE_PERCENT: usize = 10;

/// 四档 token 预算权重，**由近到远** `4:3:2:1`；从远往近数即 `1:2:3:4`
pub const MEMORY_INDEX_TIER_WEIGHTS: [usize; 4] = [4, 3, 2, 1];

/// 未配置压缩阈值时的名义基准（prompt token）
pub const MEMORY_INDEX_NOMINAL_CONTEXT_TOKENS: usize = 20_000;

/// token 预算下限：极小窗口模型也要有可用索引（不是条数下限）
pub const MEMORY_INDEX_MIN_BUDGET_TOKENS: usize = 500;

/// 压缩阈值 → 索引块 token 预算（纯函数，编译期求常量与运行时取值共用一套算法）
pub const fn memory_index_budget_tokens(threshold_tokens: usize) -> usize {
    let effective = if threshold_tokens == 0 {
        MEMORY_INDEX_NOMINAL_CONTEXT_TOKENS
    } else {
        threshold_tokens
    };
    let raw = effective * MEMORY_INDEX_CONTEXT_SHARE_PERCENT / 100;
    if raw < MEMORY_INDEX_MIN_BUDGET_TOKENS {
        MEMORY_INDEX_MIN_BUDGET_TOKENS
    } else {
        raw
    }
}

/// 预算 → 四档 token 配额；除不尽的余数优先补给更近的档位
pub const fn memory_index_tier_quotas(budget_tokens: usize) -> [usize; 4] {
    let mut weight_sum = 0;
    let mut i = 0;
    while i < 4 {
        weight_sum += MEMORY_INDEX_TIER_WEIGHTS[i];
        i += 1;
    }
    let mut quotas = [0usize; 4];
    let mut assigned = 0;
    let mut i = 0;
    while i < 4 {
        quotas[i] = budget_tokens * MEMORY_INDEX_TIER_WEIGHTS[i] / weight_sum;
        assigned += quotas[i];
        i += 1;
    }
    let mut rest = budget_tokens - assigned;
    let mut i = 0;
    while rest > 0 {
        quotas[i % 4] += 1;
        rest -= 1;
        i += 1;
    }
    quotas
}

/// 层间衰减系数 r：越远一档，单段保留字符数按 1/r 递减
pub const MEMORY_INDEX_DECAY_RATIO: usize = 2;

/// 全文层「一段」的基准字符数
pub const MEMORY_INDEX_SEGMENT_BASE_CHARS: usize = 128;

/// 单段保留字符数下限：再短就只剩几个字，失去索引价值
pub const MEMORY_INDEX_SEGMENT_MIN_CHARS: usize = 24;

/// 全文档安全阀：单条压缩摘要再长也不整条灌进索引（需要全文用 `query_memory` 反查）
///
/// ⚠️ 这里是**字符口径**，只约束「单条索引行长什么样」，不是输入预算：总量由
/// `MemoryIndexPlan::build` 的 `budget_tokens`（压缩阈值派生的 token 份额）加
/// `line_cost` 逐行计价裁决；小脑侧的摘要另走 `snippet_budget` 的 token 占比。
/// 保持绝对值是有意的 —— 索引行是 `query_memory(ids)` 反查的稳定把手，同一条
/// 记忆不该在不同容量 provider 上渲染成不同文本。别当成「字符预算漏改」改成占比。
pub const MEMORY_INDEX_FULL_MAX_CHARS: usize = 512;

/// 记忆索引层级：按摘要保留量递减
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryIndexTier {
    /// 全文 = 补全元信息后的 `to_prompt_summary`（受安全阀上限保护）
    Full,
    /// 前 / 中 / 后各取一段
    ThreeParts,
    /// 开头 + 结尾
    TwoParts,
    /// 仅开头
    OnePart,
}

impl MemoryIndexTier {
    /// 四档由近到远，容量/天界/段长都按这个顺序对齐
    pub const ALL: [Self; 4] = [Self::Full, Self::ThreeParts, Self::TwoParts, Self::OnePart];

    /// 档位序号：0 = 最近（保留最多）
    pub const fn ordinal(self) -> usize {
        match self {
            Self::Full => 0,
            Self::ThreeParts => 1,
            Self::TwoParts => 2,
            Self::OnePart => 3,
        }
    }

    /// 取几段；`Full` 返回 0 表示不走分段逻辑
    pub const fn parts(self) -> usize {
        match self {
            Self::Full => 0,
            Self::ThreeParts => 3,
            Self::TwoParts => 2,
            Self::OnePart => 1,
        }
    }

    /// 该档每段保留字符数（衰减系数折算，带下限）
    pub fn segment_chars(self) -> usize {
        let divisor = MEMORY_INDEX_DECAY_RATIO
            .saturating_pow(self.ordinal() as u32)
            .max(1);
        (MEMORY_INDEX_SEGMENT_BASE_CHARS / divisor).max(MEMORY_INDEX_SEGMENT_MIN_CHARS)
    }
}

/// 秒级时间戳 → 本地绝对时间（"2026-11-05 14:32"）
fn format_absolute_time(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "时间未知".to_string())
}

/// 相对时间标签（**只用于索引行元信息**）：以锚点 T0 为基准的离散天数桶。
///
/// 不用 `now()`、不用分钟级精度：T0 在同一次唤醒链内固定，离散桶只在跨天
/// （质变）时变化，避免逐轮字节级漂移。锚点之前的未来记忆返回 `None`。
fn relative_age_label(created_secs: i64, anchor_secs: i64) -> Option<String> {
    let delta = anchor_secs - normalize_timestamp_secs(created_secs);
    if delta < 0 {
        return None;
    }
    let days = delta / MEMORY_INDEX_DAY_SECS;
    Some(match days {
        0 => "当天".to_string(),
        1 => "昨天".to_string(),
        2..=30 => format!("{days}天前"),
        _ => format!("{}个月前", days / 30),
    })
}

/// 统一时间量级：消息 `created_at` 是毫秒、短期记忆 `created_at` 是秒。
/// 阈值 1e11（约公元 5138 年）足以分清两种量级，又不会误伤正常的秒级时间。
pub fn normalize_timestamp_secs(ts: i64) -> i64 {
    if ts > 100_000_000_000 { ts / 1000 } else { ts }
}

/// 上层算好的记忆索引计划：与 `&[Memory]`（**时间正序**）一一对应的档位。
///
/// `None` = 本轮不进索引块（预算耗尽 / 锚点之后的新记忆 / 空摘要）。
/// 计划是纯函数产物：不读时钟、不查库，只依赖调用方给出的预算、锚点与成本函数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryIndexPlan {
    pub tiers: Vec<Option<MemoryIndexTier>>,
    /// 相对时间标签的锚点 T0（秒）；`None` = 无锚点兜底（不标注相对时间）
    pub anchor_ts: Option<i64>,
    /// 预算是否已装满：`true` = 最后一档配额耗尽、`build` 提前收尾；
    /// `false` = 候选集全被消费完仍未装满 ⇒ 还需要更早的记忆（翻页信号）。
    pub saturated: bool,
}

impl MemoryIndexPlan {
    /// 分页信号：当前候选集还喂不饱预算 ⇒ 该再查一页更早的记忆。
    ///
    /// ⚠️ 只能由「预算未装满」驱动，不能由「不足 N 条」驱动：条数不是上限，
    /// 一条超长摘要就能填满预算，而 100 条短摘要可能仍未装满。
    pub fn needs_more(&self) -> bool {
        !self.saturated
    }

    /// 字符预算驱动的阶梯式加载：从最新往最旧累加，`4` 份配额用满切 `3` 份，
    /// 再满切 `2` 份、`1` 份，最后一档也满即停止（更早的记忆不进块）。
    ///
    /// - `anchor_ts`：T0，通常是触发本次唤醒的入站消息时间（毫秒/秒均可）。
    ///   `created_at > T0` 一律不进块 ⇒ 同一次唤醒链内索引块逐字节冻结。
    /// - `budget_tokens`：`memory_index_budget_tokens(压缩阈值)`。
    /// - `line_cost`：**总结阶段抽出的统一 token 估算口径**（`estimate_tokens`），
    ///   由 domain 层注入；Models 层不反向依赖 service，也不自带第二套估算。
    ///
    /// ⚠️ 单行可能超出所在档配额（极长摘要 + 极小窗口）：此时仍让这一行落地，
    /// 随后立即切档/停止，绝不跳过它 —— 否则最新一条会凭空消失。
    pub fn build<F>(
        memories: &[Memory],
        anchor_ts: Option<i64>,
        budget_tokens: usize,
        line_cost: F,
    ) -> Self
    where
        F: Fn(&str) -> usize,
    {
        let anchor = anchor_ts.map(normalize_timestamp_secs);
        let quotas = memory_index_tier_quotas(budget_tokens);
        let mut tiers = vec![None; memories.len()];
        let mut ordinal = 0usize;
        let mut used = 0usize;

        for index in (0..memories.len()).rev() {
            let memory = &memories[index];
            let MemoryPo::ShortTerm(st) = &memory.po else {
                continue;
            };
            if anchor.is_some_and(|a| normalize_timestamp_secs(st.created_at) > a) {
                continue;
            }

            loop {
                let tier = MemoryIndexTier::ALL[ordinal];
                let Some(line) = memory
                    .po
                    .to_index_prompt_at(tier, tier.segment_chars(), anchor)
                else {
                    break; // 空摘要：不占预算，也不阻塞后续条目
                };
                let cost = line_cost(&line);
                if used + cost > quotas[ordinal] {
                    if ordinal + 1 < MemoryIndexTier::ALL.len() {
                        ordinal += 1;
                        used = 0;
                        continue; // 同一行在新档重新渲染、重新计价
                    }
                    // 最后一档：该档还没落地任何行时仍放这一条，避免整块为空
                    // （极小预算 / 单行超配额场景）；随后立即停止翻更早的记忆。
                    if used == 0 {
                        tiers[index] = Some(tier);
                    }
                    return Self {
                        tiers,
                        anchor_ts: anchor,
                        saturated: true, // 预算装满：不必再翻更早的页
                    };
                }
                tiers[index] = Some(tier);
                used += cost;
                break;
            }
        }

        Self {
            tiers,
            anchor_ts: anchor,
            saturated: false, // 候选集被吃光了：还需要更早的记忆才可能装满
        }
    }

    /// 未注入计划时的兜底：同样按预算累加，只是没有 T0（不冻结、不标注相对时间）。
    /// 生产路径必须走 [`Self::build`]；这里只保证旧调用方与单测不丢行。
    pub fn fallback<F>(memories: &[Memory], budget_tokens: usize, line_cost: F) -> Self
    where
        F: Fn(&str) -> usize,
    {
        Self::build(memories, None, budget_tokens, line_cost)
    }

    /// 第 `index` 条的档位；缺项按「不进索引块」处理
    pub fn tier_at(&self, index: usize) -> Option<MemoryIndexTier> {
        self.tiers.get(index).copied().flatten()
    }
}

/// 按字符切分摘要：保留 `parts` 段，每段 `unit` 字符，段间以 `…` 标注省略。
///
/// - 中文按 `char` 切，UTF-8 安全，不会切在字节中间；
/// - 摘要本身短到「全取也不比分段多」时直接返回全文，避免出现无意义的省略号；
/// - 中段以中心对齐取，保证长文本的中部信息也能进索引。
fn segmented_summary(summary: &str, parts: usize, unit: usize) -> String {
    let chars: Vec<char> = summary.chars().collect();
    let len = chars.len();
    if unit == 0 || parts == 0 || len <= unit.saturating_mul(parts) {
        return summary.to_string();
    }
    let head: String = chars[..unit].iter().collect();
    let tail: String = chars[len - unit..].iter().collect();
    match parts {
        1 => format!("{head}…"),
        2 => format!("{head}…{tail}"),
        3 => {
            let mid_start = (len - unit) / 2;
            let middle: String = chars[mid_start..mid_start + unit].iter().collect();
            format!("{head}…{middle}…{tail}")
        }
        _ => summary.to_string(),
    }
}

impl ShortTermMemoryIndexPo {
    /// 索引行的元信息前缀（**不含列表符号**、不含摘要正文）。
    ///
    /// ⚠️ 元信息**不分级**：`[id] 绝对时间 ts=Unix秒（相对时间） #tags · task=xxx`
    /// 是最基础的信息，四档逐字节相同。`[id]` 是 `query_memory(ids)` 反查全文的
    /// 唯一把手；`ts=` 是机器可读的绝对时间戳（可直接做时间窗过滤 / 排序，
    /// 不依赖本地时区、也不用反推格式化日期），可读时间负责消歧，相对时间
    /// 让模型不必自己做日期减法。
    ///
    /// `relative` 由上层按 T0 算好传入（`None` = 无锚点场景，不标注），
    /// 本方法保持纯函数、不读时钟。只让摘要正文衰减 ⇒ 一条记忆跨档时
    /// 字节变化面最小，且永远可被精确反查。
    fn index_meta(&self, relative: Option<&str>) -> String {
        let created_secs = normalize_timestamp_secs(self.created_at);
        let mut s = format!(
            "[{}] {} ts={}",
            self.id,
            format_absolute_time(created_secs),
            created_secs
        );
        if let Some(label) = relative {
            s.push_str(&format!("（{label}）"));
        }
        let tags = flatten_tags(&self.tags);
        if !tags.is_empty() {
            let labelled = tags
                .split_whitespace()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join(" ");
            s.push(' ');
            s.push_str(&labelled);
        }
        if let Some(task_id) = self.task_id.as_deref().filter(|id| !id.is_empty()) {
            s.push_str(&format!(" · task={task_id}"));
        }
        s
    }
}

impl LongTermKnowledgeNodePo {
    /// 索引行的元信息前缀（**不含列表符号**、不含正文）。
    ///
    /// 与 [`ShortTermMemoryIndexPo::index_meta`] 同一职责、同一「元信息不分级」
    /// 不变量：id / 名称 / 类型 / 标签四档逐字节相同，只有正文随档位衰减。
    /// ⚠️ `[知识节点 {id}]` 恒在行首：`search_memory(seed_node_ids=[...])`
    /// 是 Agent 沿关系边展开的唯一把手；没有 id 就得先按关键词再搜一次，
    /// 白白多一轮工具调用。本方法不读时钟、不查库，纯打包。
    fn index_meta(&self) -> String {
        let labelled = flatten_tags(&self.user_tags)
            .split_whitespace()
            .map(|tag| format!("#{tag}"))
            .collect::<Vec<_>>()
            .join(" ");
        let suffix = if labelled.is_empty() {
            String::new()
        } else {
            format!(" · {labelled}")
        };
        format!(
            "[知识节点 {}] {} · {}{}",
            self.id, self.node_name, self.node_type, suffix
        )
    }

    /// 正文口径：`node_description` 优先，空则回退 `summary`。
    ///
    /// 放在 PO 里，避免每个渲染点各写一次 `if is_empty()` 还写出不同优先级。
    fn prompt_body(&self) -> &str {
        if self.node_description.is_empty() {
            &self.summary
        } else {
            &self.node_description
        }
    }

    /// 全文行：元信息 + 完整正文（不截断，供精选命中的少量条目使用）
    pub fn to_prompt_summary(&self) -> Option<String> {
        let body = self.prompt_body().trim();
        if body.is_empty() {
            return None;
        }
        Some(format!("{} · {}", self.index_meta(), body))
    }

    /// 索引行：元信息 + 按档位切分的正文片段，口径与短期记忆的
    /// [`MemoryPo::to_index_prompt_at`] 一致（`Full` 受
    /// [`MEMORY_INDEX_FULL_MAX_CHARS`] 安全阀封顶，其余按 `tier.parts()` 分段，
    /// `coefficient` 是每段字符数，传 0 视为不切段）。
    ///
    /// 知识节点元信息里没有相对时间、也不做 T0 冻结（图谱节点不存在
    /// 「本次唤醒之后写入的新记忆」语义），所以这里没有 `anchor_ts` 入参。
    pub fn to_index_prompt(&self, tier: MemoryIndexTier, coefficient: usize) -> Option<String> {
        let body = self.prompt_body().trim();
        if body.is_empty() {
            return None;
        }
        let text = if tier == MemoryIndexTier::Full {
            segmented_summary(body, 1, MEMORY_INDEX_FULL_MAX_CHARS)
        } else if coefficient == 0 {
            body.to_string()
        } else {
            segmented_summary(body, tier.parts(), coefficient)
        };
        Some(format!("{} · {}", self.index_meta(), text))
    }
}

impl MemoryPo {
    /// 将记忆格式化为 Prompt 可读的**全文**行（元信息 + 完整摘要）
    ///
    /// ShortTerm / KnowledgeNode 两类实体都有打包实现：元信息各自在 PO 上生成，
    /// 正文口径也由 PO 决定，格式与【历史对话】/【相关知识】区块同源；其他类型
    /// 暂不组装到 prompt。
    /// ⚠️ 元信息不可省：`[id]` 是 `query_memory(ids=[...])` 反查详情 / 沿图查边的
    /// 唯一把手。这里保持**真·全文不截断**（供 5 条量级的参考块使用），且没有
    /// T0 锚点就不标注相对时间；索引块请走 [`Self::to_index_prompt_at`]。
    pub fn to_prompt_summary(&self) -> Option<String> {
        match self {
            MemoryPo::ShortTerm(st) => {
                let summary = st.summary.trim();
                if summary.is_empty() {
                    None
                } else {
                    Some(format!("{} {}", st.index_meta(None), summary))
                }
            }
            MemoryPo::KnowledgeNode(k) => k.to_prompt_summary(),
            // 其他类型的记忆暂时不组装到 prompt 中
            _ => None,
        }
    }

    /// 无锚点版本：供单测与不需要相对时间标注的调用方使用。
    pub fn to_index_prompt(&self, tier: MemoryIndexTier, coefficient: usize) -> Option<String> {
        self.to_index_prompt_at(tier, coefficient, None)
    }

    /// 将记忆格式化为**索引**行：层级 + 保留系数 + T0 锚点打包元信息与摘要。
    ///
    /// - `tier`：本条落在哪一档；元信息四档逐字节相同，只有摘要正文衰减。
    /// - `coefficient`：**每段**保留的字符数（越大保留越多）；由上层按
    ///   [`MemoryIndexTier::segment_chars`] / [`MEMORY_INDEX_DECAY_RATIO`] 算好传入，
    ///   方法本身不读配置。传 0 视为「不切段」，退化为全文。
    /// - `anchor_ts`：T0（秒），用于元信息的相对时间标签；`None` 则不标注。
    /// - `Full` 档不分段，但受 [`MEMORY_INDEX_FULL_MAX_CHARS`] 安全阀封顶。
    ///
    /// ⚠️ 本方法不读时钟、不查库、不依赖全局配置：档位、系数、锚点都是入参，
    /// 同一次唤醒链内 T0 固定 ⇒ 索引行逐字节稳定。
    ///
    /// `KnowledgeNode` 走 [`LongTermKnowledgeNodePo::to_index_prompt`]：`anchor_ts`
    /// 对它无意义（元信息里没有相对时间），其余档位 / 系数语义完全一致。
    /// ⚠️ 生产路径的【历史对话】索引块只喂 `ShortTerm`，所以新增这个分支不会
    /// 改变已有前缀形状；混喂知识节点时由调用方自己负责预算。
    pub fn to_index_prompt_at(
        &self,
        tier: MemoryIndexTier,
        coefficient: usize,
        anchor_ts: Option<i64>,
    ) -> Option<String> {
        let st = match self {
            MemoryPo::ShortTerm(st) => st,
            MemoryPo::KnowledgeNode(k) => return k.to_index_prompt(tier, coefficient),
            _ => return None,
        };
        let summary = st.summary.trim();
        if summary.is_empty() {
            return None;
        }
        let relative = anchor_ts.and_then(|anchor| relative_age_label(st.created_at, anchor));
        let meta = st.index_meta(relative.as_deref());
        let body = if tier == MemoryIndexTier::Full {
            segmented_summary(summary, 1, MEMORY_INDEX_FULL_MAX_CHARS)
        } else if coefficient == 0 {
            summary.to_string()
        } else {
            segmented_summary(summary, tier.parts(), coefficient)
        };
        Some(format!("{meta} {body}"))
    }
}

/// 记忆写入参数
///
/// 写入分为两阶段：
/// 1. trace 先入库（`AppendTraces`），不做向量化
/// 2. 归纳总结后再写短期记忆索引（`CreateShortTerm`），自动向量化
///
/// 长期知识节点（带可选引用）/ 关系单独走 `CreateKnowledgeNode` / `CreateRelations`
#[derive(Debug, Clone)]
pub enum MemoryCreateParams {
    /// 阶段 1：仅写 trace 细节（不向量化、不创建索引）
    AppendTraces(Vec<MemoryTrace>),

    /// 阶段 2：基于已存在的 trace 创建短期记忆索引
    /// PO 内的 trace_ids 字段已包含阶段 1 返回的 id 列表
    CreateShortTerm(ShortTermMemoryIndexPo),

    /// 长期知识节点（可选附带引用关系）
    CreateKnowledgeNode {
        node: LongTermKnowledgeNodePo,
        references: Vec<KnowledgeReferencePo>,
    },

    /// 知识关系列表
    CreateRelations(Vec<KnowledgeNodeRelationPo>),
}

/// 记忆业务实体（包含 PO + 搜索匹配信息）
/// 对齐 Skill/Tool 命名模式：Memory = MemoryPo + search_match
#[derive(Debug, Clone)]
pub struct Memory {
    pub po: MemoryPo,
    pub search_match: Option<SearchMatchInfo>,
}

impl Memory {
    /// 创建新的 Memory 业务实体
    pub fn new(po: MemoryPo) -> Self {
        Self {
            po,
            search_match: None,
        }
    }

    /// 将记忆格式化为 Prompt 可读的全文行（元信息 + 完整摘要）
    ///
    /// 委托给 MemoryPo::to_prompt_summary()（ShortTerm / KnowledgeNode 两类）
    pub fn to_prompt_summary(&self) -> Option<String> {
        self.po.to_prompt_summary()
    }

    /// 短期记忆的 `(id, summary)`；非短期记忆或空摘要返回 `None`
    ///
    /// 供 Domain 层以上的调用方提取记忆摘要，避免直接匹配内部 PO 变体。
    pub fn short_term_summary(&self) -> Option<(&str, &str)> {
        let MemoryPo::ShortTerm(po) = &self.po else {
            return None;
        };
        let summary = po.summary.trim();
        (!summary.is_empty()).then_some((po.id.as_str(), summary))
    }

    /// 将记忆格式化为索引行：层级与系数均由上层算好传入（无锚点版本）
    pub fn to_index_prompt(&self, tier: MemoryIndexTier, coefficient: usize) -> Option<String> {
        self.po.to_index_prompt(tier, coefficient)
    }

    /// 带 T0 锚点的索引行：元信息额外标注相对时间，供【历史对话】索引块使用
    pub fn to_index_prompt_at(
        &self,
        tier: MemoryIndexTier,
        coefficient: usize,
        anchor_ts: Option<i64>,
    ) -> Option<String> {
        self.po.to_index_prompt_at(tier, coefficient, anchor_ts)
    }

    /// 设置搜索匹配信息
    pub fn with_search_match(mut self, search_match: SearchMatchInfo) -> Self {
        self.search_match = Some(search_match);
        self
    }

    /// 转换为 API 响应 DTO（`search_memory` / `query_memory` 共用）
    ///
    /// ⚠️ **单一实现**：这两个 handler 此前各持一份逐字相同的拷贝，改一处漏一处。
    /// 关系边两处都用 `format!("{:?}")` 输出关系类型 —— Debug 得到的是 Rust 变体名
    /// （`"Causes"`），而前端 `zh_label_from_display` 的词表 key 是 `Display` 的
    /// snake_case（`"causes"`），查表失败原样返回 → 画布上所有连线标签退化成英文词、
    /// 详情面板「内容」显示同一串英文，等于没有信息。
    ///
    /// `score` 语义 = **相关度（0.0~1.0，越大越相关）**，由向量距离换算而来
    /// （距离 0 = 完全相似 → 1.0）。⚠️ 旧实现直接吐 `vector_distance`（越小越相似），
    /// UI 标签却写「匹配分数」，方向正好相反；且关键词命中时距离为空 →
    /// 永远显示 `N/A`。只有向量参与过命中才有分值：`traverse_graph` 展开出来的
    /// 邻居节点与关系边没有匹配过程，保持 `None`，前端应整行不渲染而不是显示 `N/A`。
    pub fn to_api_result(&self) -> MemoryResult {
        let search_match = self.search_match.as_ref().map(|m| MemorySearchMatch {
            match_type: match m.match_type {
                MatchType::Hybrid => "hybrid",
                MatchType::Vector => "vector",
                MatchType::Keyword => "keyword",
            }
            .to_string(),
            vector_distance: m.vector_distance,
            fts_rank: m.fts_rank,
        });
        let score = self
            .search_match
            .as_ref()
            .and_then(|m| m.vector_distance)
            .map(|distance| (1.0 - distance).clamp(0.0, 1.0));

        match &self.po {
            MemoryPo::Trace(trace) => MemoryResult {
                id: trace.id.clone(),
                name: None,
                content: trace.input.clone(),
                memory_type: "trace".to_string(),
                score,
                summary: None,
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: None,
                properties: None,
                system_flags: None,
                status: None,
                search_match,
            },
            // 短期记忆 PO 只有一个文本字段 summary（无独立标题/正文）：
            // content 放完整 summary；summary 置 None，由前端显示层默认取
            // content 前几行作预览，避免「内容 + 摘要」渲染出同样的文本。
            MemoryPo::ShortTerm(st) => MemoryResult {
                id: st.id.clone(),
                name: None,
                content: st.summary.clone(),
                memory_type: "short_term".to_string(),
                score,
                summary: None,
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: Some(business_tags(&st.tags)),
                properties: None,
                system_flags: None,
                status: None,
                search_match,
            },
            MemoryPo::KnowledgeNode(kn) => MemoryResult {
                id: kn.id.clone(),
                name: Some(kn.node_name.clone()),
                content: kn.node_description.clone(),
                memory_type: "knowledge_node".to_string(),
                score,
                // 空串要归一成 None：写入侧未给摘要时可能是 `""`，
                // 前端 `if let Some(summary)` 会渲染一个空的摘要块
                summary: Some(kn.summary.clone()).filter(|s| !s.trim().is_empty()),
                source_node_id: None,
                target_node_id: None,
                relation_type: None,
                weight: None,
                tags: Some(business_tags(&kn.user_tags)),
                // 属性由 `search_memory` 单独批量查询后回填（不在 `Memory` 结构内）：
                // 这里恒 None，保持 PO→DTO 的纯粹映射
                properties: None,
                // 系统标记**只读派生**：`published` 的唯一真相源是 `is_published`，
                // 不从 `user_tags` 里的残留值反推 —— 存量脏值（迁移期未清洗）不语义生效。
                // 知识节点**恒有**该字段（未发布 → 空数组）：让「没有系统标记」与
                // 「这条不是知识节点」在载荷上可区分，前端不必猜。
                system_flags: Some(if kn.is_published {
                    vec![PUBLISHED_TAG.to_string()]
                } else {
                    Vec::new()
                }),
                status: Some(node_status_label(kn.status)),
                search_match,
            },
            MemoryPo::Relation(rel) => {
                // 原文落库、展示期映射：
                // - `relation_type` 给**原文**（前端着色/词表以它为输入）
                // - `name` / `content` 给映射后的展示标签（图谱连线与 hover 用）
                // 空串归一成 `None`：画布只认非空 tag，空串会让 hover 直接失效
                // （`tag: None` → tooltip 提前 return），回退「关联」才是对的。
                let relation_key = rel.relation_type.trim().to_string();
                // 展示标签：词表内 → 中文短标签，词表外 → 原文；空类型回退「关联」。
                // 不给空标签 —— 画布只认非空 tag，空串会让连线 hover 直接失效。
                let relation_label =
                    match KnowledgeRelationType::zh_label_from_display(&relation_key) {
                        "" => "关联".to_string(),
                        label => label.to_string(),
                    };
                MemoryResult {
                    id: rel.id.clone(),
                    // 关系边在数据层只有类型一个可用字段，把展示标签同时给
                    // name/content —— 详情与 hover 才不至于显示一串英文枚举名
                    name: Some(relation_label.clone()),
                    content: relation_label,
                    memory_type: "relation".to_string(),
                    score,
                    summary: None,
                    source_node_id: Some(rel.source_node_id.clone()),
                    target_node_id: Some(rel.target_node_id.clone()),
                    relation_type: Some(relation_key).filter(|s| !s.is_empty()),
                    // 关系强度透传给图谱：线宽/浓淡据此派生，`None` = 未标注
                    weight: rel.weight,
                    tags: None,
                    properties: None,
                    system_flags: None,
                    status: Some(relation_status_label(rel.status)),
                    search_match,
                }
            }
        }
    }
}

/// 知识节点状态 → 回包 `status` 文案。
///
/// 三值：`active` / `settled` / `retired`。
/// ⚠️ 存储枚举叫 `Forgotten`（已遗忘 = 软归档），回包统一叫 `retired`（已退役）——
/// 而请求参数侧仍写 `forgotten`（见 `QueryMemoryParams::status` 词表）。
/// 两端同义不同名是刻意保留的：改任一侧都会破现有调用方。
fn node_status_label(status: MemoryStatus) -> String {
    match status {
        MemoryStatus::Active => "active",
        MemoryStatus::Settled => "settled",
        MemoryStatus::Forgotten => "retired",
    }
    .to_string()
}

/// 关系边状态 → 回包 `status` 文案。
///
/// 三值：`active` / `retired`(被替换) / `deleted`(显式软删)。
/// ⚠️ `deleted` 必须显式给出：默认检索只排除 `status = 0`，`status = 2` 的边
/// 仍会进结果，合并成 `active` 会让消费方看不出「这条已被删过」。
fn relation_status_label(status: KnowledgeRelationStatus) -> String {
    match status {
        KnowledgeRelationStatus::Active => "active",
        KnowledgeRelationStatus::Superseded => "retired",
        KnowledgeRelationStatus::Deleted => "deleted",
    }
    .to_string()
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
pub(crate) fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// 入库用的可见性控制标记。
///
/// 写节点时它同时落进 `tags` 且置冗余字段 [`LongTermKnowledgeNodePo::is_published`]
/// （供查询走索引）。它是**控制位不是业务标签**，回给前端会在卡片上多出一个
/// 没有意义的英文胶囊。
pub(crate) const PUBLISHED_TAG: &str = "published";

/// 过渡期旧列 `tags` 的重建值：业务标签 +（已发布时）`published`
///
/// `user_tags` 是唯一可写面，但 FTS5 触发器（`knowledge_node_fts_ai/_au`）与
/// 旧读侧仍吃 `tags` —— 双写必须由同一个函数产出，避免两个写入口各写一套
/// 导致「旧列与新列语义漂移」。
pub(crate) fn legacy_tags_json(
    user_tags: &[String],
    is_published: bool,
) -> common::error::Result<String> {
    let mut tags: Vec<String> = user_tags.to_vec();
    if is_published && !tags.iter().any(|t| t == PUBLISHED_TAG) {
        tags.push(PUBLISHED_TAG.to_string());
    }
    Ok(serde_json::to_string(&tags)?)
}

/// 结果侧业务标签：剔除 `published` 这类控制标记
///
/// 知识节点读 `user_tags`（唯一可写标签面）；短期记忆仍读 `tags`（没有拆分）。
/// 剥离逻辑保留为**防御**：迁移未清洗的存量节点 `user_tags` 里可能还残留
/// `published`，绝不能让它冒充业务标签回到前端。
fn business_tags(tags_json: &str) -> Vec<String> {
    parse_tags_json(tags_json)
        .into_iter()
        .filter(|t| t != PUBLISHED_TAG)
        .collect()
}

/// 将 tags JSON 数组字符串展平为空格分隔的纯文本，便于向量化
///
/// 输入示例：`["rust","memory","向量"]` → `rust memory 向量`
/// 解析失败或空数组返回空字符串
pub(crate) fn flatten_tags(tags_json: &str) -> String {
    serde_json::from_str::<Vec<String>>(tags_json)
        .unwrap_or_default()
        .join(" ")
}

/// 知识图谱推荐起点（domain 层结构）
///
/// 包含知识节点 PO + 关联度数统计信息，
/// 由 DAL 层 `recommend_seed_nodes` 方法返回。
#[derive(Debug, Clone)]
pub struct SeedNodeRecommendation {
    /// 知识节点 PO
    pub node: LongTermKnowledgeNodePo,
    /// 关联度数（入边 + 出边总数）
    pub degree: usize,
    /// 入边数（被其他节点引用次数）
    pub incoming_count: usize,
    /// 出边数（引用其他节点次数）
    pub outgoing_count: usize,
}

/// 知识图谱全量数据（domain 层结构）
///
/// 由 DAL 层 `get_knowledge_graph` 返回，供全局点线视图一次拉取：
/// - 节点 = 全部活跃知识节点（R1 口径：仅知识节点 + 关系边，不含短期记忆/trace）
/// - 边 = 双端活跃的关系边（R2 口径：端点任一不在活跃节点集内的边一律丢弃）
/// - 度数 = 仅按生效边统计（R4 口径：in + out 仅活跃边）
#[derive(Debug, Clone)]
pub struct KnowledgeGraphData {
    /// 全部活跃知识节点
    pub nodes: Vec<LongTermKnowledgeNodePo>,
    /// 双端活跃的关系边
    pub edges: Vec<KnowledgeNodeRelationPo>,
    /// 节点连接度（key = 节点 ID，value = (入边数, 出边数)），仅计生效边
    pub degrees: HashMap<String, (usize, usize)>,
    /// 边方向映射（key = 边 ID，value = "directed"/"undirected"）
    ///
    /// 三期方案 a′：服务端 DAL 聚合时按本体词表 resolve 逐边带出方向，
    /// 前端零词表映射（架构不变式）。词表外/解析兜底一律 "undirected"。
    pub edge_directions: HashMap<String, String>,
}
#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
