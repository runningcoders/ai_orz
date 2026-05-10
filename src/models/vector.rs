//! 向量搜索通用数据结构
//!
//! 包含向量索引参数、匹配元信息、搜索结果包装器
//! 以及可向量化实体 Trait 接口

// ==================== 向量索引创建/更新参数 ====================

/// 向量索引创建/更新参数（通用，所有 DAO 复用）
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
    pub fn new(content: &str, vector: Vec<f32>, model_provider_id: String, embedding_model: String) -> Self {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchType {
    /// 仅向量语义匹配
    Vector,
    /// 仅关键词文本匹配
    Keyword,
    /// 同时命中两种搜索策略（双重匹配）
    Hybrid,
}

impl Default for MatchType {
    fn default() -> Self {
        MatchType::Vector
    }
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
}

// ==================== 向量搜索结果包装器 ====================

/// 搜索结果包装器（通用，所有 DAO 复用）
///
/// 包含业务 PO 对象和匹配元信息，支持向量/关键词/混合多种搜索策略
#[derive(Debug, Clone)]
pub struct SearchResult<T> {
    /// 业务 PO 对象
    pub entity: T,
    /// 匹配元信息
    pub match_info: SearchMatchInfo,
}

// ==================== 可向量化实体 Trait ====================

/// ✅ 可向量化实体 Trait
///
/// 实现这个 Trait 的实体，表示它支持被向量索引
/// 所有向量相关的业务逻辑都封装在实体内部
pub trait Vectorizable {
    // ===== 必须实现 =====

    /// 生成待向量化的文本内容
    ///
    /// 由实体自己决定：哪些字段需要被向量化？
    /// 例如 Skill 可能是 name + description，Memory 是 content
    fn vectorize_text(&self) -> String;

    /// 向量集合名称（对应 vss_{collection} 表）
    fn vector_collection() -> &'static str
    where
        Self: Sized;

    // ===== 默认实现（不需要重写） =====

    /// 计算内容哈希（默认 SHA256）
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
