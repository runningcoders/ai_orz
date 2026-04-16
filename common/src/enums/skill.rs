//! Skill 相关枚举定义

use serde::{Deserialize, Serialize};

/// 技能状态
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "INTEGER")]
pub enum SkillStatus {
    /// 已过期/已废弃，不再推荐使用，但保留历史记录
    Expired = 0,
    /// 可用，正式沉淀完成，可以正常使用
    Available = 1,
    /// 待沉淀，还在迭代中，不推荐在正式场景使用
    Pending = 2,
}

impl Default for SkillStatus {
    fn default() -> Self {
        SkillStatus::Pending
    }
}

impl From<i64> for SkillStatus {
    fn from(v: i64) -> Self {
        match v as i32 {
            0 => SkillStatus::Expired,
            1 => SkillStatus::Available,
            2 => SkillStatus::Pending,
            _ => SkillStatus::Pending,
        }
    }
}

impl SkillStatus {
    /// 转换为 i32 存储到数据库
    pub fn to_i32(self) -> i32 {
        self as i32
    }
}
