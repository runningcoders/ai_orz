//! Agent related enums

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Agent 状态（生命周期管理）
///
/// 对应一个人的职业生涯：**出生 → 学习/择业 → 面试 → 入职 → 离职**。
///
/// 状态流转：
/// Incubating → Interviewing → PendingOnboard → Onboarded → PendingOffboard → Offboarded
///
/// ## 绑定发生在「边」上，而不是「状态」里
///
/// 状态只是结果，能力获取发生在**流转的那一刻**（详见
/// `HrDomainImpl::transition_status` 的按边分发）：
///
/// - `create_agent` → **Incubating**：出生自带，只装神经工具/技能（BASE_AGENT_PACKS）；
/// - **Incubating → Interviewing**：完成职业生涯选择，按 `roles ∪ capabilities` 匹配
///   安装个人工具包/技能包（学完了才去面试）；
/// - **Interviewing → PendingOnboard**：无副作用（预留扩展，如背调/资质校验）；
/// - **PendingOnboard → Onboarded**：真正的入职，安装组织要求的工具包/技能包。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum AgentStatus {
    /// 已删除
    Deleted = 0,
    /// 面试中
    #[default]
    Interviewing = 1,
    /// 待入职（确认入职，正在初始化）
    PendingOnboard = 2,
    /// 已入职（正常可用状态）
    Onboarded = 3,
    /// 已离职
    Offboarded = 4,
    /// 待离职（交接中，不接受新任务）
    PendingOffboard = 5,
    /// 初创（刚创建，仅持有出生自带的神经能力，尚未完成职业生涯选择）
    ///
    /// 与 `Interviewing` 的区别：Interviewing 表示「已完成职业选择、可以去面试」，
    /// Incubating 表示「还在学习期，尚未定岗」。未走完
    /// `Incubating → Interviewing` 这条边的 Agent 不会获得任何角色相关能力。
    Incubating = 6,
}

impl AgentStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            1 => Self::Interviewing,
            2 => Self::PendingOnboard,
            3 => Self::Onboarded,
            4 => Self::Offboarded,
            5 => Self::PendingOffboard,
            6 => Self::Incubating,
            _ => Self::Interviewing,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<i32> for AgentStatus {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for AgentStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Agent 运行时状态（纯内存，不持久化）
///
/// 服务重启后自动重置，业务链路通过 Project/Task/Message 表可追溯。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub enum AgentRuntimeState {
    /// 空闲，可以接受新消息
    #[default]
    Idle = 0,
    /// 休息中，不接受新消息
    /// 用于恢复精力、压缩上下文、构建知识突触等
    Resting = 1,
    /// 忙碌，正在处理消息
    Busy = 2,
}

impl AgentRuntimeState {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Resting,
            2 => Self::Busy,
            _ => Self::Idle,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }

    /// 是否处于忙碌或休息状态（不可接受新消息）
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Busy | Self::Resting)
    }
}

impl From<i32> for AgentRuntimeState {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for AgentRuntimeState {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// ModelProvider status (for soft delete)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum ModelProviderStatus {
    /// Deleted (soft deleted)
    Deleted = 0,
    /// Normal (available / enabled)
    #[default]
    Normal = 1,
    /// Disabled (created but not enabled; embedding providers pending switch)
    Disabled = 2,
}

impl ModelProviderStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            1 => Self::Normal,
            2 => Self::Disabled,
            _ => Self::Normal,
        }
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl From<i32> for ModelProviderStatus {
    fn from(v: i32) -> Self {
        Self::from_i32(v)
    }
}

impl From<i64> for ModelProviderStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}
