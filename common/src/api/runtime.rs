//! Runtime API 请求/响应 DTO
//!
//! Agent 运行时状态查询、取消思考、运行中 Agent 列表。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{PagedResult, PaginationParams};
use crate::enums::ThinkingIntensity;

/// GET /agents/{id}/runtime-status 请求（path 参数：id）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RuntimeStatusRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// GET /agents/{id}/runtime-status 响应
///
/// 包含 Agent 运行时状态 + 思考运行时快照（如有）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeStatusResponse {
    /// Agent ID
    pub agent_id: String,
    /// 运行时状态："idle" / "busy" / "resting"
    pub state: String,
    /// 当前处理的消息 ID（仅 Busy 时有值）
    pub current_message_id: Option<String>,
    /// 当前关联的任务 ID
    pub task_id: Option<String>,
    /// 当前关联的项目 ID
    pub project_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
    /// 思考运行时快照（仅 Busy 时有值）
    pub think_runtime: Option<ThinkRuntimeInfo>,
}

/// 思考运行时信息（前端展示用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkRuntimeInfo {
    /// 当前 trace_id（日志检索用）
    pub trace_id: String,
    /// 场景："awaken" / "settle" / "summary" / "intent-analyze"
    pub scene: String,
    /// 当前轮次
    pub round: usize,
    /// 最大轮次
    pub max_rounds: usize,
    /// 累计输入 token
    pub tokens_input: u64,
    /// 累计输出 token
    pub tokens_output: u64,
    /// 累计总 token
    pub total_tokens: u64,
    /// 工具调用次数
    pub tool_call_count: usize,
    /// 思考状态："thinking" / "cancelled" / "finished"
    pub status: String,
    /// 思考开始时间戳（毫秒）
    pub started_at: i64,
    /// 最后更新时间戳（毫秒）
    pub last_updated_at: i64,
}

/// POST /agents/{id}/cancel-thinking 请求（path 参数：id，无 body）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CancelThinkingRequest {
    /// Agent ID
    #[param(source = "path")]
    pub id: String,
}

/// POST /agents/{id}/cancel-thinking 响应
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancelThinkingResponse {
    /// 是否成功取消（false 表示 Agent 当前未在思考）
    pub success: bool,
    /// 描述信息
    pub message: String,
}

/// GET /agents/runtime-list 请求参数（全部 query 参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RuntimeListRequest {
    /// 按状态过滤："busy" / "resting" / "idle"（不传则返回全部）
    #[param(source = "query")]
    #[serde(default)]
    pub state: Option<String>,
    /// 按任务 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按项目 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub project_id: Option<String>,
}

/// GET /agents/runtime-list 响应
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeListResponse {
    /// 运行中 Agent 列表
    pub items: Vec<RuntimeStatusResponse>,
    /// 总数
    pub total: usize,
}

/// GET /agents/think-traces 请求
///
/// `agent_id` 省略时跨 Agent 聚合（对话页多 Agent 思考流）；`agent_ids`
/// 用逗号分隔的集合进一步圈定范围（默认对话按当前任务出现过的 Agent 过滤）；
/// `project_id` / `task_id` 按轨迹条目字段过滤。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryThinkTracesRequest {
    /// 按 Agent 过滤；省略则遍历全部 Agent
    #[param(source = "query")]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// 按 Agent 集合过滤（逗号分隔的 Agent ID，如 `a,b,c`）
    ///
    /// 用于对话页「当前任务出现过的 Agent」聚合视角。GET query 不支持重复键，
    /// 故用单键逗号拼接；与 `agent_id` 同时出现时两者都生效（交集）。
    #[param(source = "query")]
    #[serde(default)]
    pub agent_ids: Option<String>,
    /// 按项目过滤（仅 2026-10-22 之后写入的轨迹带该字段）
    #[param(source = "query")]
    #[serde(default)]
    pub project_id: Option<String>,
    /// 按任务过滤（仅 2026-10-22 之后写入的轨迹带该字段）
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按 run 精确查询（awaken / settle / compact 下即 MemoryTrace.id）
    #[param(source = "query")]
    #[serde(default)]
    pub run_id: Option<String>,
    /// 思考场景（awaken / settle / summary / compact / intent-analyze）
    #[param(source = "query")]
    #[serde(default)]
    pub scene: Option<String>,
    /// created_at 下界（毫秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub started_after: Option<u64>,
    /// created_at 上界（毫秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub started_before: Option<u64>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 思考轨迹分页响应
pub type QueryThinkTracesResponse = PagedResult<ThinkTraceListItem>;

/// 思考轨迹条目（含节点 occurrence 与选路记录，展开即用，不再二次请求）
///
/// 注意：**不实现 `Eq`** —— `cerebellum` 里的 `confidence` 是 `f32`（小脑返回的
/// 概率），浮点不满足 `Eq`。`PartialEq` 足够测试断言与前端比较。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceListItem {
    /// 本次 run 的唯一 ID（awaken / settle / compact 下同时是 MemoryTrace.id）
    pub run_id: String,
    /// 发起思考的 Agent
    pub agent_id: String,
    /// 思考场景
    pub scene: String,
    /// 轨迹蓝图 ID
    pub path: String,
    /// 图执行步数
    pub steps: usize,
    /// advisor 介入次数
    pub advisor_calls: usize,
    /// 轨迹指纹
    pub fingerprint: String,
    /// 落盘时间戳（毫秒）
    pub created_at: u64,
    /// 该 run 是否已收尾（对外只回答「这条轨迹还在跑吗」）
    ///
    /// 与落盘契约的 `partial` 是同一个事实的两面：`partial` 标记「逐轮增量行 vs
    /// 收尾全量行」这个内部粒度，对外不暴露该二分语义，只给一个布尔。
    ///
    /// `finished=false`（进行中）时 `rounds` 只到已完成的最新一轮，且 `steps` /
    /// `occurrences` / `routes` 可能为空 —— 这些只有收尾行才带。前端据此显示
    /// 「进行中」，不必再靠最后一条 `decision` 反推是否还在跑。
    pub finished: bool,
    /// 关联项目 ID
    pub project_id: Option<String>,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// run 级小脑决策留痕（可能来自多段：收尾行 + chain_reply 补丁行）
    ///
    /// 空数组 = 本 run 没有小脑留痕。**前端不要把「空」解读成「未装配」**：
    /// 装配缺失由 `scene = bootstrap` / `decision = not_assembled` 的那一条显式
    /// 表达（第 0 案），空数组只说明「没有任何记录」。
    #[serde(default)]
    pub cerebellum: Vec<ThinkTraceCerebellumDto>,
    /// 轮级明细（思考文本 / 推理摘要 / 工具调用 / 裁决）
    pub rounds: Vec<ThinkTraceRoundDto>,
    /// 节点 occurrence 序列
    pub occurrences: Vec<ThinkTraceOccurrenceDto>,
    /// 选路记录
    pub routes: Vec<ThinkTraceRouteDto>,
}

/// 轨迹条目的展示 / 合并标识：`(agent_id, run_id, scene)`
///
/// `run_id` 只在同一个图 run 内唯一：Phase1 意图分析与 Phase2 唤醒**共享**
/// 主 `run_id`。因此单用 `run_id` 做合并键会把两个场景并成一条，做前端 DOM
/// key / 展开态又会把两张卡片一起展开。后端查询合并与前端列表都走这里，
/// 保证「合并键 = 展示标识」同源。
pub fn think_trace_entry_key(agent_id: &str, run_id: &str, scene: &str) -> String {
    format!("{agent_id}|{run_id}|{scene}")
}

impl ThinkTraceListItem {
    /// 本条轨迹的展示 / 合并标识，见 [`think_trace_entry_key`]
    pub fn entry_key(&self) -> String {
        think_trace_entry_key(&self.agent_id, &self.run_id, &self.scene)
    }
}

/// 该场景的思考轨迹是否对应一条可回查的记忆条目（input / output）
///
/// 只有这四类场景的 `run_id` 与 `MemoryTrace.id` 同值：唤醒 / 沉淀 / 压缩
/// 由唤醒主流程写记忆，总结退出由收尾路径写记忆；意图识别只落思考轨迹，
/// 没有可回查的条目。
pub fn think_trace_has_memory_entry(scene: &str) -> bool {
    matches!(scene, "awaken" | "settle" | "compact" | "summary")
}

/// run 级小脑决策留痕的一条记录
///
/// 与 `ThinkTraceCerebellum`（后端落盘结构）字段一一对应，不做裁剪：
/// `scene` × `decision` × `degraded` 是三维平表的三个维度，`applied` 区分
/// 「判了但没改变下游」，`latency_ms` 是 p95 时延的唯一回收点。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceCerebellumDto {
    /// 场景（entry_gate / entry_pool / thinking_strength.* / memory_relevance /
    /// skill_relevance / chain_reply / bootstrap）
    pub scene: String,
    /// 决策结论（选中的选项 / 分数 / fallback / not_assembled）
    pub decision: String,
    /// 置信度（Pick 的概率；未产出结论时为 None）
    pub confidence: Option<f32>,
    /// 降级原因（None = 正常出结论）
    pub degraded: Option<String>,
    /// 是否真正影响了下游行为（false = 判了但未生效，如 Provider 未启用强度能力）
    pub applied: bool,
    /// 本次判定的真实调用耗时（毫秒）；未发生调用时为 None
    pub latency_ms: Option<u64>,
}

/// 单轮思考明细
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceRoundDto {
    /// 轮次编号（1-based）
    pub round: usize,
    /// 本轮模型可见文本
    pub assistant_text: Option<String>,
    /// 本轮推理摘要（仅 provider 返回的可读 summary）
    pub reasoning: Vec<String>,
    /// 本轮工具调用
    pub tool_calls: Vec<ThinkTraceToolCallDto>,
    /// 本轮输入 token
    pub input_tokens: u64,
    /// 本轮输出 token
    pub output_tokens: u64,
    /// 本轮提示缓存命中的输入 token（provider 未上报时为 0）
    #[serde(default)]
    pub cached_tokens: u64,
    /// 本轮思考/推理消耗的输出 token（`output_tokens` 的子集；未上报时为 0）
    #[serde(default)]
    pub reasoning_tokens: u64,
    /// 本轮 LLM 调用耗时（毫秒）
    pub duration_ms: u64,
    /// 裁决结果：final / continue / reflect / exit
    pub decision: Option<String>,
    /// 本轮调用模型时实际下发的思考强度（0–100 分）；None = Provider 未启用该能力
    #[serde(default)]
    pub thinking_intensity: Option<ThinkingIntensity>,
}

/// 单轮内的一次工具调用
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceToolCallDto {
    /// 工具调用 trace ID（可据此查 tool_tracing 的完整入参/出参）
    pub call_id: Option<String>,
    /// 工具 wire name
    pub name: String,
    /// 入参预览（截断）
    pub arguments: String,
    /// 结果预览（截断）
    pub result: Option<String>,
    /// 执行状态：ok / error / blocked / not_found / pending
    pub status: String,
}

/// 单节点执行记录
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceOccurrenceDto {
    /// 节点名
    pub node: String,
    /// 同节点执行序号
    pub seq: usize,
    /// 图步编号
    pub step: usize,
    /// 节点产出信号
    pub signal: Option<String>,
}

/// 单次选路记录
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThinkTraceRouteDto {
    /// 图步编号
    pub step: usize,
    /// 候选节点
    pub candidates: Vec<String>,
    /// 实际选择节点
    pub chosen: String,
    /// 选路来源（unique / deterministic / advisor / fallback）
    pub via: String,
    /// 选择原因
    pub reason: Option<String>,
}

// ==================== 记忆轨迹 ====================

/// GET /agents/memory-traces 请求
///
/// `agent_id` 省略时跨 Agent 聚合（Admin 全量视图）；时间过滤单位为**秒**
/// （与 `MemoryTrace.created_at` 一致），与思考轨迹的毫秒口径刻意区分。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct QueryMemoryTracesRequest {
    /// 按 Agent 过滤；省略则遍历全部 Agent
    #[param(source = "query")]
    #[serde(default)]
    pub agent_id: Option<String>,
    /// 按任务过滤
    #[param(source = "query")]
    #[serde(default)]
    pub task_id: Option<String>,
    /// 按请求日志 ID 过滤
    #[param(source = "query")]
    #[serde(default)]
    pub log_id: Option<String>,
    /// 按 trace ID 精确查询（详情弹窗之外的列表定位）
    #[param(source = "query")]
    #[serde(default)]
    pub trace_id: Option<String>,
    /// created_at 下界（秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub created_after: Option<i64>,
    /// created_at 上界（秒，含）
    #[param(source = "query")]
    #[serde(default)]
    pub created_before: Option<i64>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 记忆轨迹分页响应
pub type QueryMemoryTracesResponse = PagedResult<MemoryTraceListItem>;

/// 记忆轨迹列表项（input/output 仅返回预览，完整内容走详情接口）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryTraceListItem {
    /// 轨迹 ID（awaken / settle / compact 下与 think trace 的 run_id 相同）
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 请求日志 ID
    pub log_id: String,
    /// 创建者用户 ID
    pub user_id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色（system / user / assistant / summary）
    pub role: String,
    /// 创建时间（秒）
    pub created_at: i64,
    /// 完成时间（秒）
    pub completed_at: Option<i64>,
    /// 输入预览（截断）
    pub input_preview: String,
    /// 输出预览（截断；中断时为空）
    pub output_preview: Option<String>,
    /// 元数据
    pub metadata: std::collections::HashMap<String, String>,
}

/// GET /agents/memory-traces/{trace_id} 请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetMemoryTraceRequest {
    /// 轨迹 ID
    #[param(source = "path")]
    pub trace_id: String,
    /// 轨迹所属 Agent（定位 `agents/{agent_id}/memory/` 目录）
    #[param(source = "query")]
    pub agent_id: String,
}

/// 记忆轨迹详情（完整输入输出）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryTraceDetail {
    /// 轨迹 ID
    pub id: String,
    /// 所属 Agent
    pub agent_id: String,
    /// 关联任务 ID
    pub task_id: Option<String>,
    /// 请求日志 ID
    pub log_id: String,
    /// 创建者用户 ID
    pub user_id: String,
    /// 所属组织 ID
    pub organization_id: String,
    /// 角色（system / user / assistant / summary）
    pub role: String,
    /// 创建时间（秒）
    pub created_at: i64,
    /// 完成时间（秒）
    pub completed_at: Option<i64>,
    /// 完整输入（Prompt）
    pub input: String,
    /// 完整输出（中断时为空）
    pub output: Option<String>,
    /// 元数据
    pub metadata: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(agent_id: &str, run_id: &str, scene: &str) -> ThinkTraceListItem {
        ThinkTraceListItem {
            agent_id: agent_id.to_string(),
            run_id: run_id.to_string(),
            scene: scene.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn entry_key_keeps_shared_run_id_apart_across_scenes() {
        // Phase1 意图与 Phase2 唤醒共享 run_id，展示 / 合并键必须按 scene 区分
        let intent = item("agent-a", "run-1", "intent-analyze");
        let awaken = item("agent-a", "run-1", "awaken");
        assert_ne!(intent.entry_key(), awaken.entry_key());

        // 同一 (agent, run, scene) 的重跑必须落到同一个键，后端才能合并片段
        assert_eq!(
            intent.entry_key(),
            item("agent-a", "run-1", "intent-analyze").entry_key()
        );
    }

    #[test]
    fn memory_entry_scenes_cover_both_compact_and_summary() {
        // 压缩与总结退出都会写记忆条目，轨迹面板两者都要能回查 input / output
        assert!(think_trace_has_memory_entry("compact"));
        assert!(think_trace_has_memory_entry("summary"));
        // 意图识别只落思考轨迹，没有可回查的条目
        assert!(!think_trace_has_memory_entry("intent-analyze"));
    }
}
