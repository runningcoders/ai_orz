//! Model Provider related API request/response DTOs - shared between backend and frontend

use crate::api::PaginationParams;
use crate::enums::{ModelCapability, ModelProviderStatus, ProviderType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateModelProviderRequest {
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model capability
    pub capability: ModelCapability,
    /// Model name
    pub model_name: String,
    /// API Key
    pub api_key: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
}

/// Create Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model name
    pub model_name: String,
    /// Description
    pub description: Option<String>,
    /// Created timestamp
    pub created_at: i64,
}

/// Model Provider list item response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelProviderListItem {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model capability
    pub capability: ModelCapability,
    /// Model name
    pub model_name: String,
    /// Description
    pub description: Option<String>,
    /// Provider status (0=deleted, 1=normal)
    pub status: i32,
    /// Created timestamp
    pub created_at: i64,
}

/// Get Model Provider request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
    /// 是否附带模型调用统计
    #[param(source = "query")]
    pub with_model_call_stats: Option<bool>,
    /// 统计时间范围起始（毫秒），需与 end_time 配对使用
    #[param(source = "query")]
    pub stats_start_time: Option<i64>,
    /// 统计时间范围结束（毫秒），需与 start_time 配对使用
    #[param(source = "query")]
    pub stats_end_time: Option<i64>,
    /// 统计时序查询粒度：Hourly / Daily
    #[param(source = "query")]
    pub stats_interval: Option<String>,
}

/// Get Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model capability
    pub capability: ModelCapability,
    /// Model name
    pub model_name: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Provider status (0=deleted, 1=normal)
    pub status: i32,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
    /// 模型调用统计（可选）
    pub stats: Option<crate::models::ModelCallStats>,
}

/// Update Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,

    /// New provider name
    pub name: Option<String>,
    /// New provider type
    pub provider_type: Option<ProviderType>,
    /// New model name
    pub model_name: Option<String>,
    /// New API Key
    pub api_key: Option<String>,
    /// New custom Base URL
    pub base_url: Option<String>,
    /// New description
    pub description: Option<String>,
    /// New status (0=deleted/disabled, 1=normal/enabled)
    pub status: Option<i32>,
}

/// Update Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model capability
    pub capability: ModelCapability,
    /// Model name
    pub model_name: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Provider status (0=deleted/disabled, 1=normal/enabled)
    pub status: i32,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Test connection request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct TestConnectionRequest {
    /// Optional test prompt
    pub prompt: Option<String>,
}

/// Test connection response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestConnectionResponse {
    /// Whether the test succeeded
    pub success: bool,
    /// Model response (on success)
    pub response: Option<String>,
    /// Error message (on failure)
    pub error: Option<String>,
}

/// Test Model Provider connection request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct TestModelProviderConnectionRequest {
    /// Provider ID to test
    #[param(source = "path")]
    pub id: String,
    /// Optional test prompt
    pub prompt: Option<String>,
}

/// Call model request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CallModelRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
    /// Prompt to call
    pub prompt: String,
}

/// Call model response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CallModelResponse {
    /// Generated result
    pub result: String,
}

/// Delete Model Provider request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteModelProviderResponse {
    /// Whether deletion succeeded
    pub success: bool,
}

/// List Model Providers request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListModelProvidersRequest {
    // No parameters needed - all providers are returned
}

/// List Model Providers response
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListModelProvidersResponse {
    /// List of all model providers
    pub providers: Vec<ModelProviderListItem>,
}

/// Model Provider 通用查询请求（POST body）
///
/// 支持完整查询条件 + 分页，query 是核心查询能力。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ModelProviderQueryRequest {
    /// 按提供方类型查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<ProviderType>,
    /// 按能力类型查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<ModelCapability>,
    /// 按状态查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ModelProviderStatus>,
    /// 排除指定状态
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_status: Option<ModelProviderStatus>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Model Provider list item response alias (frontend compatibility)
pub type ListModelProvidersResponseItem = ModelProviderListItem;

/// Test Model Provider connectivity response (alias for frontend compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestModelProviderConnectionResponse {
    /// Whether the test succeeded
    pub success: bool,
    /// Test result message
    pub message: String,
    /// Model result (on success)
    pub result: Option<String>,
}

/// Switch Embedding Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct SwitchEmbeddingProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
    /// User confirmation flag (must be true)
    pub confirm: bool,
}

/// Switch Embedding Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SwitchEmbeddingProviderResponse {
    /// New provider ID
    pub id: String,
    /// New provider name
    pub name: String,
    /// Previous provider ID (if existed)
    pub previous_provider_id: Option<String>,
    /// Previous provider name (if existed)
    pub previous_provider_name: Option<String>,
    /// Rebuild status
    pub rebuild_status: String,
    /// Rebuild task ID (empty if rebuild completed synchronously)
    pub task_id: String,
}

/// Rebuild status
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildStatus {
    /// Rebuild is queued but not started yet
    Pending,
    /// Rebuild is currently running
    Running,
    /// Rebuild finished successfully
    Completed,
    /// Rebuild failed with an error
    Failed,
}

/// Rebuild progress response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RebuildProgressResponse {
    /// Task ID
    pub task_id: String,
    /// Rebuild status
    pub status: RebuildStatus,
    /// Current entity being processed (e.g., "memory", "skill")
    pub current_entity: Option<String>,
    /// Current entity index (0..total_entities)
    pub current_entity_index: usize,
    /// Total entities to rebuild
    pub total_entities: usize,
    /// Number of records processed in current entity
    pub processed_records: usize,
    /// Total records in current entity
    pub total_records: usize,
    /// Start timestamp (ms)
    pub started_at: i64,
    /// Finish timestamp (ms, optional)
    pub finished_at: Option<i64>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Get rebuild progress request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetRebuildProgressRequest {
    /// Task ID to query
    #[param(source = "query")]
    pub task_id: String,
}
