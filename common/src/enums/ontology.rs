//! 本体域枚举

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 本体词表条目状态（对齐全库软删除约定）
///
/// 只表达词表条目自身的生命周期：1 正常（可被新写入引用）/ 0 退役。
/// ⚠️ **退役 ≠ 删除**：退役词的历史图谱存量引用仍需可解释（读侧照常展示、
/// 照常参与 resolve 之外的解释），只是写侧不再允许新引用 —— 管理页下架
/// 一个关系词，不能让既有连线一夜之间"消失"或退化成无语义的裸边。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum OntologyStatus {
    /// 退役 - 0：写侧不再允许新引用，历史存量引用保留可解释
    Retired = 0,
    /// 正常 - 1：可被新写入引用
    #[default]
    Active = 1,
}

impl From<i32> for OntologyStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => OntologyStatus::Retired,
            _ => OntologyStatus::Active,
        }
    }
}

impl From<i64> for OntologyStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl OntologyStatus {
    /// 转换为 i32 用于数据库存储
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}
