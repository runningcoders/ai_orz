//! Organization 持久化对象
//!
//! 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_ORGANIZATIONS`]

use common::constants::utils;
use common::enums::{OrganizationStatus, OrganizationScope};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// OrganizationPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrganizationPo {
    /// 组织 ID
    pub id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述
    pub description: String,
    /// 组织外网访问基础 URL
    ///
    /// 例如：`https://ai-orz.example.com/org/acme`
    /// 用于前端生成访问链接
    pub base_url: Option<String>,
    /// 状态枚举
    pub status: OrganizationStatus,
    /// 组织范围枚举（区分本地/远程，用于多节点网络扩展）
    pub scope: OrganizationScope,
    /// 创建人
    pub created_by: String,
    /// 修改人
    pub modified_by: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

impl OrganizationPo {
    /// 创建新的 OrganizationPo
    pub fn new(
        id: String,
        name: String,
        description: String,
        base_url: Option<String>,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        Self {
            id,
            name,
            description,
            base_url,
            status: OrganizationStatus::default(),
            scope: OrganizationScope::default(),
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }
}
