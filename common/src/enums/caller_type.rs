//! Caller type enum - 标识 RequestContext 的触发方身份

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// 调用方类型 - 标识谁触发了本次操作
///
/// 语义：caller_type 表示"谁触发了本次操作"，与 stats 的 operator_type 语义一致。
/// 数值与 MessageRole 对齐（0=User, 1=Agent, 2=System），但语义层次不同：
/// - MessageRole 是消息字段（from_role/to_role）
/// - CallerType 是 ctx 字段（标识当前操作链路的触发方）
///
/// 设置时机：
/// - HTTP 中间件：默认 User（JWT 验证通过的用户请求）
/// - Consumer rebuild_context：根据 message.from_role() 设置
/// - Producer/Cron/A2A callback：显式 System
/// - enrich_ctx 链路：不覆盖（透传入口设置的值）
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum CallerType {
    /// User - 用户触发（HTTP 请求、用户消息）
    #[default]
    User = 0,
    /// Agent - Agent 触发（Agent 主动调用工具、发送消息）
    Agent = 1,
    /// System - 系统触发（Cron、A2A 回调、AOP 调度、后台轮询）
    System = 2,
}

impl CallerType {
    /// 转为字符串（用于 stats operator_type）
    pub fn as_str(&self) -> &'static str {
        match self {
            CallerType::User => "User",
            CallerType::Agent => "Agent",
            CallerType::System => "System",
        }
    }
}

impl From<i32> for CallerType {
    fn from(v: i32) -> Self {
        match v {
            0 => CallerType::User,
            1 => CallerType::Agent,
            2 => CallerType::System,
            _ => CallerType::default(),
        }
    }
}

impl From<CallerType> for i32 {
    fn from(c: CallerType) -> i32 {
        c as i32
    }
}

impl std::fmt::Display for CallerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
