//! Memory 相关枚举
//!
//! - `MemoryStatus` - 记忆状态（活跃/已遗忘）

#![deny(missing_docs)]

/// 记忆状态
///
/// 用于短期记忆索引和长期知识节点的状态管理：
/// - `Forgotten` = 0：已遗忘（归档，默认不参与检索，降低信息过载）
/// - `Active` = 1：活跃（正常可检索，参与问答和搜索）
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum MemoryStatus {
    /// 已遗忘 - 0，默认过滤不查询，保留数据可恢复
    Forgotten = 0,
    /// 活跃 - 1，正常可检索
    Active = 1,
}

impl From<i64> for MemoryStatus {
    fn from(v: i64) -> Self {
        match v as i32 {
            0 => MemoryStatus::Forgotten,
            1 => MemoryStatus::Active,
            _ => MemoryStatus::Active, // 默认活跃
        }
    }
}
