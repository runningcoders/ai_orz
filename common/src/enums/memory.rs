//! Memory 相关枚举
//!
//! - `MemoryStatus` - 记忆状态（活跃/已遗忘）

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};

/// 记忆状态
///
/// 用于短期记忆索引和长期知识节点的状态管理：
/// - `Forgotten` = 0：已遗忘（归档，默认不参与检索，降低信息过载）
/// - `Active` = 1：活跃（正常可检索，参与问答和搜索）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MemoryStatus {
    /// 已遗忘 - 0，默认过滤不查询，保留数据可恢复
    Forgotten = 0,
    /// 活跃 - 1，正常可检索
    Active = 1,
}

impl Default for MemoryStatus {
    fn default() -> Self {
        MemoryStatus::Active // 默认活跃
    }
}

impl From<i32> for MemoryStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => MemoryStatus::Forgotten,
            1 => MemoryStatus::Active,
            _ => MemoryStatus::default(),
        }
    }
}

impl From<i64> for MemoryStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl MemoryStatus {
    /// Convert to i32 for database storage
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}
