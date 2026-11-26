//! Cron Trigger related enumerations shared by backend and frontend.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Cron trigger type.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TriggerType {
    /// One-time trigger.
    Once = 0,
    /// Cron expression trigger.
    #[default]
    Cron = 1,
    /// Fixed interval trigger.
    Interval = 2,
    /// Schedule trigger: 形态标签，时间规则复用 Once/Cron/Interval 三选一
    Schedule = 3,
}

impl From<i32> for TriggerType {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Once,
            1 => Self::Cron,
            2 => Self::Interval,
            3 => Self::Schedule,
            _ => Self::Cron,
        }
    }
}

impl From<i64> for TriggerType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl TriggerType {
    /// Convert the trigger type to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

/// 触发器的可用状态（列表过滤用）。
///
/// 表里没有独立 status 列：启用/取消就是 `is_enabled` 的 1/0。这里把它变成
/// 具名状态，供管理端列表以「状态列表」形式传参（默认只看启用），避免前端
/// 拉全量再本地过滤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerStatus {
    /// 正常可用
    #[default]
    Enabled,
    /// 已取消 / 暂停
    Disabled,
}

impl TriggerStatus {
    /// 映射到 `is_enabled` 的数据库取值。
    pub fn to_i32(&self) -> i32 {
        match self {
            Self::Enabled => 1,
            Self::Disabled => 0,
        }
    }
}

/// 日程查询的相对日期词。
///
/// 模型只负责选择语义（"明天"、"下周"），**不负责换算毫秒时间戳**；
/// 区间由服务端按 `server.timezone` 统一折算，避免跨天/跨月/跨年算错。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleDateTerm {
    /// 今天（默认）
    #[default]
    Today,
    /// 昨天
    Yesterday,
    /// 明天
    Tomorrow,
    /// 昨天 + 今天 + 明天（三天视图一次拿齐）
    ThreeDays,
    /// 本周一至周日
    ThisWeek,
    /// 下周一至周日
    NextWeek,
    /// 由 `from` / `to` 显式给出区间
    Custom,
}

/// 触发行为模板。
///
/// 有限枚举，不落 `consume_mode` / `order_key` / `retry_policy` 三个自由字段 ——
/// AOP 注册期硬禁「Sync + ordered」，允许数据自由组合必然造出非法记录。
/// 表里只存本枚举，消费语义由模板静态决定。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum TriggerBehavior {
    /// 同步轻量维护（系统默认触发器）：Sync、不 ordered、失败不标记下轮重捞
    #[default]
    SyncMaintenance = 0,
    /// 异步通知用户：Async + ordered，nack 重投
    AsyncNotify = 1,
    /// 异步唤醒 Agent：Async + ordered，nack 重投
    AsyncAgentWakeup = 2,
}

impl From<i32> for TriggerBehavior {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::AsyncNotify,
            2 => Self::AsyncAgentWakeup,
            _ => Self::SyncMaintenance,
        }
    }
}

impl From<i64> for TriggerBehavior {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl TriggerBehavior {
    /// Convert to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

/// 触发记录创建者类型。独立于 `CallerType`：后者是「谁触发本次操作」，
/// 本枚举是「谁创建了这条调度规则」，语义不可混用。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum CreatorType {
    /// 用户创建
    #[default]
    User = 0,
    /// Agent 创建（自主日程）
    Agent = 1,
    /// 系统创建（默认触发器）
    System = 2,
}

impl From<i32> for CreatorType {
    fn from(v: i32) -> Self {
        match v {
            1 => Self::Agent,
            2 => Self::System,
            _ => Self::User,
        }
    }
}

impl From<i64> for CreatorType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

impl CreatorType {
    /// Convert to i32 for database storage.
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}
