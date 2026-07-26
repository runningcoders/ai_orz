//! Seed 配置迁移相关 API DTO

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 列出 seeds/ 目录请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListSeedsRequest {}

/// 单个 seed 文件信息
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SeedFileInfo {
    /// 文件名（不含路径，含 .json 后缀）
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最后修改时间戳（毫秒）
    pub modified_at: i64,
    /// 是否为系统默认模板（基于文件名前缀判断）
    pub is_default: bool,
}

/// 列出 seeds/ 响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSeedsResponse {
    /// 文件列表
    pub data: Vec<SeedFileInfo>,
    /// 文件总数
    pub total: u64,
}

/// 读取 seed 文件请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetSeedFileRequest {
    /// 文件名（含 .json 后缀）
    #[param(source = "path")]
    pub name: String,
}

/// 读取 seed 文件响应（返回完整 JSON 内容）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetSeedFileResponse {
    /// 文件名（含 .json 后缀）
    pub name: String,
    /// 文件完整 JSON 内容
    pub content: String,
    /// 文件大小（字节）
    pub size: u64,
}

/// 保存当前组织配置到文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SaveSeedRequest {
    /// 文件名（不含路径，会自动加 .json 后缀）
    pub name: String,
    /// 描述（可选，写入快照 metadata）
    pub description: Option<String>,
}

/// 保存响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SaveSeedResponse {
    /// 文件名（含 .json 后缀）
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
}

/// 加载 seed 文件请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct LoadSeedRequest {
    /// 文件名（含 .json 后缀）
    #[param(source = "path")]
    pub name: String,
    /// 导入策略
    pub strategy: ImportStrategy,
    /// 敏感字段值（key = "{entity_type}:{entity_id}:{field}"）
    /// 导入前若快照含 PENDING_INPUT 占位符，前端必须填写后传入
    #[serde(default)]
    pub sensitive_values: std::collections::HashMap<String, String>,
}

/// 导入策略
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
pub enum ImportStrategy {
    /// 保留快照中的 ID（适合同组织回滚/恢复）
    #[default]
    PreserveIds,
    /// 生成新 ID（适合跨组织迁移）
    RegenerateIds,
    /// 仅预演，不实际写入，返回 diff 报告
    DryRun,
    /// 仅新建不存在的，已存在的跳过
    SkipExisting,
}

/// 加载响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadSeedResponse {
    /// 新建的实体数量
    pub created: usize,
    /// 更新的实体数量
    pub updated: usize,
    /// 跳过的实体数量（SkipExisting 策略）
    pub skipped: usize,
    /// DryRun 模式下的 diff 报告（非 DryRun 时为 None）
    ///
    /// 注：使用 `serde_json::Value` 而非具体 `SeedDiff` 类型，因为 `SeedDiff`
    /// 引用了 `ai_orz` crate 中定义的实体结构（`OrganizationDef` 等），
    /// 而 `common` crate 不能依赖 `ai_orz`。Handler 层在返回时通过
    /// `serde_json::to_value(&seed_diff)` 转换。
    pub diff: Option<serde_json::Value>,
}

/// 删除 seed 文件请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteSeedFileRequest {
    /// 文件名（含 .json 后缀）
    #[param(source = "path")]
    pub name: String,
}

/// 删除响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteSeedFileResponse {
    /// 是否删除成功
    pub success: bool,
}

/// Diff 请求（文件 vs DB）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DiffSeedRequest {
    /// 文件名（含 .json 后缀）
    #[param(source = "path")]
    pub name: String,
}

/// 两个文件之间 diff 请求
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DiffFilesRequest {
    /// 基准文件名
    #[param(source = "query")]
    pub base: String,
    /// 目标文件名
    #[param(source = "query")]
    pub target: String,
}

/// 应用默认模板请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ApplyDefaultSeedRequest {
    /// 导入策略
    pub strategy: ImportStrategy,
    /// 敏感字段值
    #[serde(default)]
    pub sensitive_values: std::collections::HashMap<String, String>,
}

/// 获取默认模板请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetDefaultSeedRequest {}
