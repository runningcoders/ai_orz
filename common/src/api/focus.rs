//! 专注浮层 · 实时思考图协议（后端 → 前端 SSE 线格式）
//!
//! 设计口径（2026-11-23 对齐）：
//! - **骨架优先**：先让「思考图」本身事件化（节点 / 阶段 / 边 / 选路），
//!   细节（工具调用、单轮思考输出、小脑决策）再挂到骨架上；
//! - 全部事件都是**运行时消息**，不落消息库；SSE 断开即丢（历史仍有思考轨迹可查）；
//! - 一张图 = 一个 `graph_id`。嵌套图（awaken 的 deliberate 内部跑轮级环）
//!   各自持有独立 `graph_id`，前端按 `graph_id` 折叠展示。
//!
//! ⚠️ 这里只放**数据形状**，不构造 ID / 时间戳：构造发生在服务端
//! `models::events`，避免把 `chrono` / `uuid` 带进 wasm 侧。

use serde::{Deserialize, Serialize};

/// 图中一个节点的静态目录信息（run 开始时下发一次）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusGraphNode {
    /// 节点定义 id（蓝图内唯一）
    pub id: String,
    /// 阶段标签（stimulus / triage / deliberate / consolidate ...）
    pub phase: String,
    /// 一句话用途（给前端做节点说明）
    pub purpose: String,
    /// 冲突仲裁优先级（大者优先）
    pub priority: i32,
}

/// 图中一条静态边（`on = None` 表示任意信号）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusGraphEdge {
    /// 起点节点
    pub from: String,
    /// 终点节点
    pub to: String,
    /// 匹配的前驱信号标签；`None` = 任意
    pub on: Option<String>,
}

/// 思考图骨架事件
///
/// 顶层字段是**信封**（谁、哪张图、哪次 run）；`action` 是本次发生的动作。
/// AOP 发布时会在 JSON 顶层再注入 `kind`（`agent.think.graph`）、`event_id`、
/// `created_at` 等元字段，前端据此分派。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkGraphEvent {
    /// 事件 id（服务端生成）
    pub event_id: String,
    /// 图所属 Agent
    pub agent_id: String,
    /// 思考场景：awaken / settle / 轮级图的 scene 字符串
    pub scene: String,
    /// 图 id（嵌套图各自独立）
    pub graph_id: String,
    /// 本次 run id（与思考轨迹同源）
    pub run_id: String,
    /// 蓝图 id
    pub path: String,
    /// 任务维度（可空）
    pub task_id: Option<String>,
    /// 项目维度（可空）
    pub project_id: Option<String>,
    /// 服务端事件时间戳（毫秒）
    pub created_at: i64,
    /// 本次动作
    #[serde(flatten)]
    pub action: ThinkGraphAction,
}

/// 骨架动作：只描述「图上发生了什么」，不含业务细节
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ThinkGraphAction {
    /// run 开始：下发整张图的节点 / 边目录，前端据此先画出空图
    RunStarted {
        /// 入口节点
        entry: String,
        /// 全部节点目录
        nodes: Vec<FocusGraphNode>,
        /// 全部边目录
        edges: Vec<FocusGraphEdge>,
        /// 刺激摘要（用户输入的前若干字）
        stimulus: String,
    },
    /// 节点开始执行
    NodeStarted {
        /// 节点 id
        node: String,
        /// 阶段标签
        phase: String,
        /// 第几步（1 基）
        step: usize,
        /// 该节点第几次出现（1 基）
        seq: usize,
        /// 节点用途
        purpose: String,
    },
    /// 节点执行结束（含失败）
    NodeFinished {
        /// 节点 id
        node: String,
        /// 第几步
        step: usize,
        /// 该节点第几次出现
        seq: usize,
        /// 产出的信号标签（done / fail / final / tool_calls ...）
        signal: Option<String>,
        /// 是否失败
        failed: bool,
        /// 节点耗时（毫秒）
        duration_ms: u64,
    },
    /// 仲裁器选出了下一步
    RouteDecided {
        /// 第几步之后做出的选择
        step: usize,
        /// 合法候选
        candidates: Vec<String>,
        /// 选中节点
        chosen: String,
        /// 选路来源（unique / deterministic / advisor / fallback）
        via: String,
        /// 选路理由
        reason: String,
    },
    /// run 结束（正常或异常）
    RunFinished {
        /// 总步数
        steps: usize,
        /// 异常信息；`None` = 正常结束
        error: Option<String>,
        /// run 总耗时（毫秒）
        duration_ms: u64,
    },
}

/// 单轮思考输出（细节层：挂在 `think` 节点上）
///
/// 轮次级而非 token 级：token 流会淹没骨架，且骨架已经能表达「正在想」。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkOutputEvent {
    /// 事件 id
    pub event_id: String,
    /// Agent id
    pub agent_id: String,
    /// 思考场景
    pub scene: String,
    /// 图 id
    pub graph_id: String,
    /// run id
    pub run_id: String,
    /// 唤醒级 0 基轮次（与思考轨迹同源）
    pub round: usize,
    /// 推理摘要（多段时按换行拼接）
    pub reasoning: String,
    /// 助手输出预览
    pub content: String,
    /// 服务端事件时间戳（毫秒）
    pub created_at: i64,
}

/// 工具调用开始（细节层：调用意图 + 参数）
///
/// 结束事件复用既有 `ToolExecEvent`（`agent.tool.executed`），不再新造一份。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStartedEvent {
    /// 事件 id
    pub event_id: String,
    /// Agent id
    pub agent_id: String,
    /// 调用 id（与结束事件同值，用于配对）
    pub call_id: String,
    /// 工具 id
    pub tool_id: String,
    /// 工具名
    pub tool_name: String,
    /// 模型自述的调用意图（`internal_intent`）
    pub intent: Option<String>,
    /// 实际业务入参（已摘除 `internal_intent`）
    pub args: serde_json::Value,
    /// 任务维度（可空）
    pub task_id: Option<String>,
    /// 项目维度（可空）
    pub project_id: Option<String>,
    /// 服务端事件时间戳（毫秒）
    pub created_at: i64,
}

/// 小脑决策摘要（一道题一行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CerebellumAnswerBrief {
    /// 题目 id
    pub qid: String,
    /// 结论的一行描述
    pub summary: String,
}

/// 小脑（System One）决策事件
///
/// `outcome`：`ok` / `timeout` / `call_error`；`degraded` 为降级原因字符串
/// （与 `common::api::cerebellum::DegradedReason` 同词表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CerebellumDecidedEvent {
    /// 事件 id
    pub event_id: String,
    /// Agent id
    pub agent_id: String,
    /// 本批涉及的场景
    pub scenes: Vec<String>,
    /// 结论：ok / timeout / call_error
    pub outcome: String,
    /// 降级原因（`None` = 正常拿到答案）
    pub degraded: Option<String>,
    /// 端到端耗时（毫秒）
    pub latency_ms: u64,
    /// 各题结论摘要
    pub answers: Vec<CerebellumAnswerBrief>,
    /// 服务端事件时间戳（毫秒）
    pub created_at: i64,
}
