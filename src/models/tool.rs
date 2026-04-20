//! Tool 持久化对象

use common::enums::{ToolProtocol, ToolStatus};
use rig::tool::ToolDyn;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Tool 持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolPo {
    /// 工具 ID
    pub id: String,
    /// 工具名称（唯一）
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 工具协议类型
    pub protocol: ToolProtocol,
    /// 协议配置（JSON）
    pub config: serde_json::Value,
    /// 参数 JSON Schema（动态工具必填，内置工具可选）
    pub parameters_schema: Option<serde_json::Value>,
    /// 工具状态
    pub status: ToolStatus,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
    /// 创建者
    pub created_by: Option<String>,
    /// 更新者
    pub updated_by: Option<String>,
}

/// Tool 完整实体
///
/// 包含持久化元数据 + 实际可执行的 ToolDyn trait 对象
pub struct Tool {
    /// 持久化元数据
    pub po: ToolPo,
    /// 实际可执行工具（rig trait 对象）
    pub tool: Box<dyn ToolDyn + Send + Sync>,
}

// Manual Debug implementation - skip the dyn ToolDyn field
impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("po", &self.po)
            .field("tool", &format_args!("Box<dyn ToolDyn + Send + Sync>"))
            .finish()
    }
}

// Manual Clone implementation - Agent derives Clone, but dyn ToolDyn can't be cloned
// In practice, Agent is wrapped in Arc when shared, so this unreachable is safe
impl Clone for Tool {
    fn clone(&self) -> Self {
        unreachable!("Tool cannot be cloned due to dyn Trait object. Use Arc<Agent> for sharing.")
    }
}

impl ToolPo {
    /// 创建新 ToolPo（如果 id 为空自动生成 Uuid v7）
    pub fn new(
        id: String,
        name: String,
        description: String,
        protocol: ToolProtocol,
        config: serde_json::Value,
        parameters_schema: Option<serde_json::Value>,
        creator: Option<String>,
    ) -> Self {
        let id = if id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp();
        Self {
            id,
            name,
            description,
            protocol,
            config,
            parameters_schema,
            status: ToolStatus::Enabled,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    /// 更新时间戳和修改者
    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp();
        self.updated_by = modifier;
    }
}
