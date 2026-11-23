//! 工作台场景投影 DTO（前后端共用契约）
//!
//! 语义层（presence / attention）由后端计算，前端只负责表现，因此本文件只放传输结构，
//! 不含任何判定规则。投影是事件流的 fold 结果：v1 经 HTTP 返回当前值，v2 经 SSE
//! 推送同一结构的增量，消费方不变。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 场景投影请求
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, Params)]
pub struct GetSceneProjectionRequest {
    /// 工位数上限，缺省 8，最大 8
    #[param(source = "query")]
    pub seat_limit: Option<u32>,
}

/// 场景投影响应
///
/// `generated_at` 是唯一的新鲜度判据：前端据此提示「数据停在 X 分钟前」，
/// 不允许在无事件时假装更新。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SceneProjectionResponse {
    /// 投影生成时间（毫秒）
    pub generated_at: i64,
    /// 进程内单调序号，重启归零；仅用于增量对齐，不用于判断新鲜度
    pub seq: u64,
    /// 按 attention 优先级排序后的实体，已按工位数截断
    pub entities: Vec<SceneEntity>,
    /// 需要用户出手的实体数（不受截断影响）
    pub attention_total: u32,
    /// 全量统计（不受截断影响）
    pub stats: SceneStats,
}

/// 场景实体
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SceneEntity {
    /// 与 Agent / Task / Project 的业务 id 对齐
    pub id: String,
    /// 实体类型
    pub kind: SceneEntityKind,
    /// 展示名称
    pub display_name: String,
    /// 语义维度，前端渲染只依赖这一层
    pub dims: PresenceDims,
    /// 显示用标签；开放集合，后端新增事件类型只增标签，不改渲染
    pub labels: Vec<StateLabel>,
    /// 需要用户介入的程度
    pub attention: SceneAttention,
    /// 最近一次活动时间（毫秒）
    pub last_activity_at: i64,
}

/// 场景实体类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum SceneEntityKind {
    /// Agent
    #[default]
    Agent,
    /// 任务
    Task,
    /// 项目
    Project,
}

/// 语义维度：与具体事件类型解耦，前端按维度渲染，不按状态枚举穷举
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PresenceDims {
    /// 能否打扰（休息 / 停滞 / 未运行为否）
    pub interruptible: bool,
    /// 当前类别
    pub category: SceneCategory,
    /// 卡在等什么；仅 `Waiting` 有值
    pub blocked_on: Option<BlockReason>,
    /// 当前 category 的起始时间（毫秒）
    pub since: i64,
}

/// 场景类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum SceneCategory {
    /// 空闲
    #[default]
    Idle,
    /// 在思考 / 调工具
    Working,
    /// 休息（沉淀）
    Resting,
    /// 在等用户回话（仅空闲/休息态成立；忙碌不算等待）
    Waiting,
    /// 忙碌超阈值无状态变化，仅作提示，不进 NeedsYou
    Stalled,
    /// 不在运行时列表中
    Offline,
}

/// 卡点原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum BlockReason {
    /// 等用户回复
    #[default]
    UserReply,
    /// 等审批
    Approval,
    /// 等外部结果
    ExternalResult,
    /// 未知
    Unknown,
}

/// 需要用户介入的程度
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
pub enum SceneAttention {
    /// 无需介入
    #[default]
    None,
    /// 值得一眼
    Info,
    /// 等你出手
    NeedsYou,
}

/// 显示用状态标签
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StateLabel {
    /// 稳定键：think_round / tool_call / waiting_user …
    pub key: String,
    /// 展示文案
    pub text: String,
    /// 标签产生时间（毫秒）
    pub at: i64,
}

/// 全量统计，口径与工位数截断无关
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SceneStats {
    /// Agent 总数
    pub agent_total: u32,
    /// 忙碌数
    pub working: u32,
    /// 空闲数
    pub idle: u32,
    /// 休息数
    pub resting: u32,
    /// 等你回话数
    pub waiting: u32,
    /// 疑似停滞数
    pub stalled: u32,
    /// AOP 队列总待处理数
    pub queue_backlog: u64,
}
