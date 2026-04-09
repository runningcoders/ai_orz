//! Organization 持久化对象
//!
//! 对应 SQL 建表语句：[`crate::pkg::storage::sql::SQLITE_CREATE_TABLE_ORGANIZATIONS`]

use crate::pkg::constants::utils;
use serde::{Deserialize, Serialize};

/// OrganizationPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub base_url: String,
    /// 状态：0 = 禁用，1 = 启用
    pub status: i32,
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
        base_url: String,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp();
        Self {
            id,
            name,
            description,
            base_url,
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }
}
