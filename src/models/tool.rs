//! Tool 持久化对象

use common::enums::{ToolProtocol, ToolStatus};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Tool 持久化对象
///
/// 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_TOOLS`]
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
