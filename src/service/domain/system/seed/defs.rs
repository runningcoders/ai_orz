//! Seed 配置迁移核心数据结构
//!
//! 快照只保留业务实体定义（配置层），不包含运行时数据（消息、任务、stats、日志、向量索引）
//! 敏感字段（password_hash / api_key）永远不导出，使用 PENDING_INPUT 占位符

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 敏感字段占位符常量
pub const PENDING_INPUT: &str = "PENDING_INPUT";
/// 继承当前 DB 值（用于回滚场景，由 handler 传入当前值给纯函数解析）
pub const INHERIT_CURRENT: &str = "INHERIT_CURRENT";
/// 随机生成（导入时由 handler 生成并显示一次）
pub const RANDOM_GENERATE: &str = "RANDOM_GENERATE";

/// Seed 快照根结构
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedSnapshot {
    /// 快照格式版本
    pub version: String,
    /// 生成时间戳（毫秒）
    pub generated_at: i64,
    /// 快照描述（可选）
    pub description: Option<String>,
    /// 源组织 ID（用于追踪）
    pub source_organization_id: String,
    /// 组织定义
    pub organization: OrganizationDef,
    /// 用户列表
    pub users: Vec<UserDef>,
    /// 模型 Provider 列表
    pub model_providers: Vec<ModelProviderDef>,
    /// Agent 列表
    pub agents: Vec<AgentDef>,
    /// Skill 列表
    pub skills: Vec<SkillDef>,
}

impl SeedSnapshot {
    /// 当前快照格式版本
    pub const CURRENT_VERSION: &'static str = "1.0.0";
}

/// 组织定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub status: i32,
    pub scope: i32,
}

/// 用户定义（不含 password_hash）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UserDef {
    pub id: String,
    pub organization_id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    /// 密码占位符（PENDING_INPUT / INHERIT_CURRENT / RANDOM_GENERATE）
    pub password_ref: String,
    pub role: i32,
    pub status: i32,
}

/// 模型 Provider 定义（api_key 使用占位符）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelProviderDef {
    pub id: String,
    pub name: String,
    pub provider_type: i32,
    pub model_name: String,
    pub capability: i32,
    /// API Key 占位符（PENDING_INPUT / INHERIT_CURRENT）
    pub api_key_ref: String,
    pub base_url: Option<String>,
    pub description: Option<String>,
    pub config: String,
    pub status: i32,
}

/// Agent 定义
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AgentDef {
    pub id: String,
    pub name: String,
    /// 角色标签数组
    pub roles: Vec<String>,
    pub description: String,
    pub capabilities: Vec<String>,
    pub soul: String,
    /// 关联的 ModelProvider ID（引用 model_providers 中的某项）
    pub model_provider_id: String,
    /// 运行时配置（JSON）
    pub runtime_config: String,
    pub status: i32,
    pub kind: i32,
}

/// Skill 定义（不含文件内容，仅元数据）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category: String,
    pub parent_skill_id: String,
    pub author_id: String,
    pub author_type: i32,
    pub status: i32,
    /// 相对 base_data_path 的技能目录路径（导入时复制目录）
    pub content_path: String,
}

// ==================== Diff 结构 ====================

/// Diff 报告
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedDiff {
    pub meta: DiffMeta,
    pub summary: DiffSummary,
    pub organization: Option<DiffEntry<OrganizationDef>>,
    pub users: Vec<DiffEntry<UserDef>>,
    pub model_providers: Vec<DiffEntry<ModelProviderDef>>,
    pub agents: Vec<DiffEntry<AgentDef>>,
    pub skills: Vec<DiffEntry<SkillDef>>,
}

/// Diff 元信息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffMeta {
    pub kind: DiffKind,
    pub base_source: String,
    pub target_source: String,
    pub compared_at: i64,
}

/// Diff 类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub enum DiffKind {
    /// 文件 vs 当前 DB
    FileVsDb,
    /// DB vs 文件（反向）
    DbVsFile,
    /// 文件 vs 文件
    FileVsFile,
}

/// Diff 摘要统计
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DiffSummary {
    pub new_count: usize,
    pub updated_count: usize,
    pub same_count: usize,
    pub removed_count: usize,
}

/// 单个实体的 diff
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum DiffEntry<T> {
    Same {
        id: String,
        current: T,
    },
    Updated {
        id: String,
        current: T,
        snapshot: T,
        changes: Vec<FieldChange>,
    },
    New {
        id: String,
        snapshot: T,
    },
    Removed {
        id: String,
        current: T,
    },
}

/// 字段级变更
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FieldChange {
    /// 字段路径（如 "name"、"config.max_context_length"）
    pub field: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
}
