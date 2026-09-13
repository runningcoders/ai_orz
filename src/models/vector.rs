//! 向量搜索通用数据结构
//!
//! 包含向量行数据、搜索命中结果、索引参数、以及可向量化实体 Trait 接口

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

// ==================== 向量 Payload（原始过滤信息）与谓词过滤 ====================

/// 向量过滤字段（与 VectorPayload 的列一一对应）
///
/// 注意：tags 落 payload 但不参与谓词下推（回表兜底），故不在此枚举中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorField {
    OrgId,
    AgentId,
    ProjectId,
    TaskId,
    FromId,
    ToId,
    EntityType,
    Status,
    IsPublished,
}

impl VectorField {
    /// payload 列名（LanceDB 平铺列名 / SQL 字段名 / JSON key，三处统一）
    pub fn column_name(&self) -> &'static str {
        match self {
            Self::OrgId => "org_id",
            Self::AgentId => "agent_id",
            Self::ProjectId => "project_id",
            Self::TaskId => "task_id",
            Self::FromId => "from_id",
            Self::ToId => "to_id",
            Self::EntityType => "entity_type",
            Self::Status => "status",
            Self::IsPublished => "is_published",
        }
    }
}

/// 谓词过滤值
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Str(String),
    Bool(bool),
}

/// 向量谓词过滤表达式（结构化；各后端自行翻译执行）
///
/// 定位：召回优化，不是正确性保证——下推不了的业务条件由回业务表过滤兜底
#[derive(Debug, Clone)]
pub enum VectorFilter {
    /// 字段等值匹配（payload 字段为 None 的行不匹配）
    Eq(VectorField, FilterValue),
    /// 字段 IN 集合匹配
    In(VectorField, Vec<FilterValue>),
    /// 全部满足（AND）
    All(Vec<Self>),
    /// 任一满足（OR；用于共享可见性 agent_id = 自己 OR is_published）
    Any(Vec<Self>),
}

impl VectorFilter {
    /// 内存求值（InMemory / Hnsw 后端复用）
    pub fn matches(&self, payload: &VectorPayload) -> bool {
        match self {
            Self::Eq(f, v) => payload.get(f).as_ref() == Some(v),
            Self::In(f, vs) => payload.get(f).is_some_and(|v| vs.contains(&v)),
            Self::All(fs) => fs.iter().all(|f| f.matches(payload)),
            Self::Any(fs) => fs.iter().any(|f| f.matches(payload)),
        }
    }

    /// 翻译为 SQL 谓词（LanceDB 直接用列名；SqliteVss 用 json_extract 包装字段名）
    ///
    /// `col` 闭包负责字段名渲染：Lance 传 `|f| f.column_name().to_string()`，
    /// SqliteVss 传 `|f| format!("json_extract(payload_json, '$.{}')", f.column_name())`
    pub fn to_sql_expr<F: Fn(&VectorField) -> String>(&self, col: F) -> String {
        fn escape_sql_str(s: &str) -> String {
            s.replace('\'', "''")
        }
        fn render_value(v: &FilterValue) -> String {
            match v {
                FilterValue::Str(s) => format!("'{}'", escape_sql_str(s)),
                FilterValue::Bool(b) => b.to_string(),
            }
        }
        // 内部递归统一走 `&F`，避免闭包引用层层嵌套触发泛型递归上限
        fn render<F: Fn(&VectorField) -> String>(f: &VectorFilter, col: &F) -> String {
            match f {
                VectorFilter::Eq(f, v) => format!("{} = {}", col(f), render_value(v)),
                VectorFilter::In(f, vs) => {
                    let items: Vec<String> = vs.iter().map(render_value).collect();
                    format!("{} IN ({})", col(f), items.join(", "))
                }
                VectorFilter::All(fs) => {
                    if fs.len() == 1 {
                        render(&fs[0], col)
                    } else {
                        format!(
                            "({})",
                            fs.iter()
                                .map(|f| render(f, col))
                                .collect::<Vec<_>>()
                                .join(" AND ")
                        )
                    }
                }
                VectorFilter::Any(fs) => {
                    if fs.len() == 1 {
                        render(&fs[0], col)
                    } else {
                        format!(
                            "({})",
                            fs.iter()
                                .map(|f| render(f, col))
                                .collect::<Vec<_>>()
                                .join(" OR ")
                        )
                    }
                }
            }
        }
        render(self, &col)
    }
}

/// 向量 Payload：随向量落库的原始过滤信息（统一宽结构）
///
/// 「列」全局统一（所有 collection 同 schema，4 个后端共用）；「行」由各 PO 的
/// `Vectorizable::vector_payload()` 自治填充（信息专家原则，与 vectorize_text 同源）。
/// 新增可过滤字段的路径：本结构加列 → 相关 PO 填充 → 对应 vector DAO 转译白名单加映射。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct VectorPayload {
    /// 组织 ID（Message 有；Agent/Task/Project/Tool/Skill 的 PO 无此字段暂 None）
    pub org_id: Option<String>,
    /// Agent ID（Memory / Message）
    pub agent_id: Option<String>,
    /// 项目 ID（Task / Message）
    pub project_id: Option<String>,
    /// 任务 ID（Memory / Message）
    pub task_id: Option<String>,
    /// 消息发送方（Message）
    pub from_id: Option<String>,
    /// 消息接收方（Message）
    pub to_id: Option<String>,
    /// 实体子类型（node_type / tool_type / message_type 等 String 类型字段统一映射）
    pub entity_type: Option<String>,
    /// 状态（枚举暂以稳定字符串存储；第一期不参与转译白名单，仅落库）
    pub status: Option<String>,
    /// 是否已发布（KnowledgeNode 共享可见性）
    pub is_published: Option<bool>,
    /// 标签（JSON 数组字符串，与 PO tags 同构；第一版不参与谓词下推）
    pub tags: Option<String>,
}

impl VectorPayload {
    /// 按字段取值（求值用）
    pub fn get(&self, field: &VectorField) -> Option<FilterValue> {
        match field {
            VectorField::OrgId => self.org_id.clone().map(FilterValue::Str),
            VectorField::AgentId => self.agent_id.clone().map(FilterValue::Str),
            VectorField::ProjectId => self.project_id.clone().map(FilterValue::Str),
            VectorField::TaskId => self.task_id.clone().map(FilterValue::Str),
            VectorField::FromId => self.from_id.clone().map(FilterValue::Str),
            VectorField::ToId => self.to_id.clone().map(FilterValue::Str),
            VectorField::EntityType => self.entity_type.clone().map(FilterValue::Str),
            VectorField::Status => self.status.clone().map(FilterValue::Str),
            VectorField::IsPublished => self.is_published.map(FilterValue::Bool),
        }
    }

    /// payload 哈希（sha256(稳定 JSON)；serde_json 按 struct 字段顺序序列化，稳定）
    pub fn hash(&self) -> String {
        sha256::digest(serde_json::to_string(self).unwrap_or_default())
    }
}

/// 重索引决策（三态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexDecision {
    /// 文本与 payload 均未变化：跳过
    Skip,
    /// 仅 payload 变化：只刷新 payload 列，不重新调 embedding
    PayloadOnly,
    /// 向量化文本变化：完整重索引（重新 embed）
    FullReindex,
}

// ==================== 向量存储通用数据结构 ====================

/// 向量元数据（持久化）
#[derive(Debug, Clone, Encode, Decode)]
pub struct VectorMeta {
    pub content_hash: String,
    pub embedding_model: String,
    pub indexed_at: i64,
    pub expire_at: Option<i64>,
}

/// 向量行结构体（对应 vector_metadata 表，完整的一行数据）
#[derive(Debug, Clone, Encode, Decode)]
pub struct VectorRow {
    /// 业务表 ID（如 skill_id, memory_id）
    pub id: String,
    /// 向量数据
    pub vector: Vec<f32>,
    /// 元数据
    pub meta: VectorMeta,
}

/// 向量搜索命中结果（完整行数据 + 相似度距离）
#[derive(Debug, Clone)]
pub struct VectorSearchHit {
    /// 完整的向量行数据
    pub row: VectorRow,
    /// 相似度距离（越小越相似）
    pub distance: f32,
}

/// 向量集合（完整可序列化）
#[derive(Debug, Clone, Encode, Decode)]
pub struct VectorCollection {
    pub dimensions: i32,
    pub entries: Vec<VectorRow>,
}

impl VectorCollection {
    pub fn new(dimensions: i32) -> Self {
        Self {
            dimensions,
            entries: Vec::new(),
        }
    }
}

// ==================== 向量索引创建/更新参数 ====================\n\n/// 向量索引创建/更新参数（通用，所有 DAO 复用）
#[derive(Debug, Clone)]
pub struct VectorIndexParams {
    /// 向量数据
    pub vector: Vec<f32>,
    /// 内容哈希（用于判断是否需要重索引）
    pub content_hash: String,
    /// 生成该向量的 ModelProvider ID
    pub model_provider_id: String,
    /// 使用的模型名称
    pub embedding_model: String,
    /// 过期时间（None 表示永不过期）
    pub expire_at: Option<i64>,
}

impl VectorIndexParams {
    /// 从向量化文本和向量创建索引参数
    pub fn new(
        content: &str,
        vector: Vec<f32>,
        model_provider_id: String,
        embedding_model: String,
    ) -> Self {
        let content_hash = sha256::digest(content);
        Self {
            vector,
            content_hash,
            model_provider_id,
            embedding_model,
            expire_at: None,
        }
    }

    /// 设置过期时间
    pub fn with_expire_at(mut self, expire_at: i64) -> Self {
        self.expire_at = Some(expire_at);
        self
    }
}

/// 搜索匹配类型（支持混合策略）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchType {
    /// 仅向量语义匹配
    #[default]
    Vector,
    /// 仅关键词文本匹配
    Keyword,
    /// 同时命中两种搜索策略（双重匹配）
    Hybrid,
}

/// 搜索匹配元信息（不含泛型，可嵌入任何业务实体）
///
/// 支持混合搜索策略，同时记录向量匹配和关键词匹配的元数据
#[derive(Debug, Clone, Default)]
pub struct SearchMatchInfo {
    /// 匹配类型（向量/关键词/混合）
    pub match_type: MatchType,
    /// 向量相似度距离（越小越相似，0.0-1.0），关键词匹配时为 None
    pub vector_distance: Option<f32>,
    /// 关键词匹配命中的字段列表（如 ["name", "description"]），向量匹配时为 None
    pub keyword_fields: Option<Vec<String>>,
    /// 使用的 Embedding 模型名称（向量匹配时有值）
    pub embedding_model: Option<String>,
    /// 索引创建时间（Unix 时间戳，毫秒）
    pub indexed_at: Option<i64>,
    /// 内容哈希（用于判断是否过时）
    pub content_hash: Option<String>,
    /// FTS5 BM25 相关性评分（越小越相关，仅关键词命中时有值）
    pub fts_rank: Option<f32>,
}

// ==================== 向量搜索结果包装器 ====================\n\n/// 搜索结果包装器（通用，所有 DAO 复用）
///
/// 包含业务 PO 对象和匹配元信息，支持向量/关键词/混合多种搜索策略
#[derive(Debug, Clone)]
pub struct SearchResult<T> {
    /// 业务 PO 对象
    pub entity: T,
    /// 匹配元信息
    pub match_info: SearchMatchInfo,
}

// ==================== 可向量化实体 Trait ====================\n\n/// ✅ 可向量化实体 Trait
///
/// 实现这个 Trait 的实体，表示它支持被向量索引
/// 所有向量相关的业务逻辑都封装在实体内部
pub trait Vectorizable: Send + Sync {
    // ===== 必须实现 =====\n\n    /// 生成待向量化的文本内容
    ///
    /// 由实体自己决定：哪些字段需要被向量化？
    /// 例如 Skill 可能是 name + description，Memory 是 content
    fn vectorize_text(&self) -> String;

    /// 向量集合名称（对应 vss_{collection} 表）
    fn vector_collection() -> &'static str
    where
        Self: Sized;

    // ===== 默认实现（不需要重写） =====\n\n    /// 计算内容哈希（默认 SHA256）
    fn vector_content_hash(&self) -> String {
        sha256::digest(self.vectorize_text())
    }

    /// 向量过期时间（可选覆盖，默认永不过期）
    fn vector_expire_at(&self) -> Option<i64> {
        None
    }

    /// 判断内容是否变化，是否需要重索引
    fn needs_reindex(&self, existing_hash: &str) -> bool {
        self.vector_content_hash() != existing_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_match_info_with_fts_rank() {
        let info = SearchMatchInfo {
            match_type: MatchType::Keyword,
            fts_rank: Some(-1.5),
            ..Default::default()
        };
        assert_eq!(info.match_type, MatchType::Keyword);
        assert_eq!(info.fts_rank, Some(-1.5));
        assert!(info.vector_distance.is_none());
    }

    #[test]
    fn test_search_match_info_default_fts_rank_is_none() {
        let info = SearchMatchInfo::default();
        assert!(info.fts_rank.is_none());
    }

    // ==================== VectorPayload / VectorFilter 测试 ====================

    fn sample_payload() -> VectorPayload {
        VectorPayload {
            agent_id: Some("agent-1".to_string()),
            is_published: Some(false),
            ..Default::default()
        }
    }

    #[test]
    fn test_vector_payload_hash_stable() {
        let a = sample_payload();
        let b = sample_payload();
        assert_eq!(a.hash(), b.hash());
        // 任意字段变化 → hash 变化
        let mut c = sample_payload();
        c.is_published = Some(true);
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn test_vector_filter_matches_eq() {
        let f = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
        assert!(f.matches(&sample_payload()));
        let f2 = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-2".into()));
        assert!(!f2.matches(&sample_payload()));
        // payload 缺字段（None）不匹配任何值
        let f3 = VectorFilter::Eq(VectorField::ProjectId, FilterValue::Str("p".into()));
        assert!(!f3.matches(&sample_payload()));
    }

    #[test]
    fn test_vector_filter_matches_any_or() {
        // agent_id = agent-1 OR is_published = true（共享可见性）
        let f = VectorFilter::Any(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
        ]);
        assert!(f.matches(&sample_payload())); // 命中第一支（自己且未发布）
        let mut other = VectorPayload {
            agent_id: Some("agent-2".into()),
            is_published: Some(true),
            ..Default::default()
        };
        assert!(f.matches(&other)); // 命中第二支（别人的已发布）
        other.is_published = Some(false);
        assert!(!f.matches(&other)); // 别人的未发布 → 不可见
    }

    #[test]
    fn test_vector_filter_to_sql_expr() {
        let f = VectorFilter::All(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("a'1".into())),
            VectorFilter::Any(vec![
                VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
                VectorFilter::In(
                    VectorField::EntityType,
                    vec![FilterValue::Str("concept".into())],
                ),
            ]),
        ]);
        let sql = f.to_sql_expr(|field| field.column_name().to_string());
        // 单引号转义（SQL 注入防护）
        assert!(sql.contains("agent_id = 'a''1'"));
        assert!(sql.contains("is_published = true"));
        assert!(sql.contains("entity_type IN ('concept')"));
        assert!(sql.contains(" AND ") && sql.contains(" OR "));
    }
}
