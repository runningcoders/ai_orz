//! 轮级思考图轨迹（`think_path` run trace）的落盘出口与查询入口
//!
//! 与 `tool_tracing` 同一套「显式初始化 + 未初始化即 no-op」模式：
//! 生产环境在 [`crate::pkg::init_all`] 注入 base_data_path；单元测试不初始化，
//! 因此不会往 `~/.ai_orz` 写文件（测试隔离红线）。
//!
//! 落盘是「逐轮增量 + 收尾全量」两段式（`agents/{agent_id}/think_trace/{YYYYMMDD}.jsonl`）：
//! - 每轮定型（think → dispatch → evaluate）后追加一行 `partial=true`，只含该轮明细；
//! - run 收尾追加一行 `partial=false`，含完整 rounds 与节点 occurrence 序列、选路来源。
//!
//! 查询按 `(agent_id, run_id, scene)` 合并：收尾行只替换**本片段**的增量 rounds、
//! 片段之间拼接续编号，因此前端在 run 未结束时查询也能看到已完成轮次（半成品），
//! 异常中断也不会整条丢失。
//!
//! 查询侧按 **agent 维度** 分页：`agent_id` 缺省时遍历 `agents/*/think_trace`，
//! 供对话页「多 Agent 聚合流」使用；`project_id` / `task_id` 是 entry 字段过滤
//! （2026-10-22 起新写入的轨迹才有值，历史轨迹该字段为空）。
//!
//! 同一图 run 可能因新输入重跑思考循环（意图修订 / 执行回退），每次重跑追加一行；
//! 查询在分页前按 `(agent_id, run_id, scene)` 合并成一个动作，避免轨迹面板把
//! 同一个动作展示成多个条目、各自统计轮次。

use crate::pkg::daily_jsonl::DailyJsonlWriter;
use crate::pkg::paths;
use common::api::{PagedResult, PaginationParams};
use common::enums::ThinkingIntensity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 单次查询允许的最大条数（JSONL 扫描开销控制）
pub const MAX_THINK_TRACE_QUERY_LIMIT: usize = 100;

/// 未显式指定 limit 时的默认页大小
pub const DEFAULT_THINK_TRACE_QUERY_LIMIT: usize = 20;

static BASE_DATA_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 启动期注入基础数据目录（重复调用无副作用）
pub fn init(base_data_path: impl Into<PathBuf>) {
    let _ = BASE_DATA_PATH.set(base_data_path.into());
}

/// 追加一条轨迹
///
/// 未初始化时静默跳过并返回 `Ok(false)`——测试或独立调用场景不应产生文件副作用。
/// 落盘前补齐 `created_at`（毫秒）：查询分页与时间过滤依赖该字段，调用方漏填时
/// 由出口统一兜底，避免历史行出现「有轨迹但无法排序/过滤」的断层。
pub fn append(agent_id: &str, entry: &serde_json::Value) -> anyhow::Result<bool> {
    let Some(base) = BASE_DATA_PATH.get() else {
        return Ok(false);
    };
    let mut entry = entry.clone();
    if entry.get("created_at").is_none() {
        entry["created_at"] =
            serde_json::Value::from(chrono::Utc::now().timestamp_millis().max(0) as u64);
    }
    let writer = DailyJsonlWriter::new(paths::agent_think_trace_dir(base, agent_id));
    writer.append_no_position(&entry)?;
    Ok(true)
}

// ====================================================================
// 轨迹条目（读取侧类型）
// ====================================================================

/// 一次轮级思考循环的轨迹条目
///
/// 字段名与落盘 JSON 一一对应；`#[serde(default)]` 让历史行/字段扩展时仍可读取。
/// 唯一的例外是 `partial_round_count` —— 它是查询合并期的内部状态，不落盘。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThinkTraceEntry {
    /// 本次 run 的唯一 ID（图引擎生成）
    #[serde(default)]
    pub run_id: String,
    /// 发起本次思考的 Agent
    #[serde(default)]
    pub agent_id: String,
    /// 思考场景（awaken / settle / summary / compact / intent-analyze）
    #[serde(default)]
    pub scene: String,
    /// 轨迹使用的蓝图 ID（固定为 `think_round`）
    #[serde(default)]
    pub path: String,
    /// 图执行步数
    #[serde(default)]
    pub steps: usize,
    /// advisor 介入次数
    #[serde(default)]
    pub advisor_calls: usize,
    /// 轨迹指纹（同拓扑同来源时的去重/对比依据）
    #[serde(default)]
    pub fingerprint: String,
    /// 节点 occurrence 序列
    #[serde(default)]
    pub occurrences: Vec<ThinkTraceOccurrence>,
    /// 每次选路的候选与来源
    #[serde(default)]
    pub routes: Vec<ThinkTraceRoute>,
    /// 落盘时间戳（毫秒，append 出口兜底补齐）
    #[serde(default)]
    pub created_at: u64,
    /// 关联项目 ID（2026-10-22 起写入）
    #[serde(default)]
    pub project_id: Option<String>,
    /// 关联任务 ID（2026-10-22 起写入）
    #[serde(default)]
    pub task_id: Option<String>,
    /// 轮级明细：模型思考文本、reasoning summary、工具调用与决策
    #[serde(default)]
    pub rounds: Vec<ThinkTraceRound>,
    /// 本次 run 内的小脑决策留痕（routing / 思考强度 / 检索精选）
    ///
    /// 小脑调用点分散在 run 的不同阶段，落盘只有这里一个出口，因此由
    /// [`record_cerebellum`] 按 run 收集、收尾时 [`take_cerebellum`] 一次取走。
    /// 增量行（`partial=true`）不写该字段：它只承载轮次明细，run 级信息统一由
    /// 收尾行提供（与 `steps` / `occurrences` / `routes` 同口径）。
    #[serde(default)]
    pub cerebellum: Vec<ThinkTraceCerebellum>,
    /// 本行是否为「运行中逐轮增量」而非 run 收尾的完整行
    ///
    /// 思考循环每轮定型（think → dispatch → evaluate）后追加一行 `partial=true`，
    /// 只携带该轮的 `rounds` 明细；run 收尾再追加一行 `partial=false`，携带完整
    /// `rounds` 与图执行信息（occurrences / routes / fingerprint）。
    ///
    /// 这样前端在 run 未结束时查询也能看到已完成轮次（半成品），异常中断也
    /// 不会整条丢失。查询合并时收尾行**替换本片段**增量行累积的 rounds——两者
    /// 是同一份数据的两个粒度，拼接会导致轮次翻倍。
    #[serde(default)]
    pub partial: bool,
    /// 查询合并期内部状态：当前片段已累积的逐轮增量 `rounds` 条数（不落盘）
    ///
    /// 一次唤醒内可能因意图修订多次重跑，每个片段各自「N 行增量 + 1 行收尾」。
    /// 收尾行需要摘掉**本片段**的增量轮次再拼接，否则第二个片段的收尾行会把
    /// 第一个片段的轮次一起冲掉。这个计数精确反映实际收到的增量行数（写失败
    /// 只告警，可能少于收尾行的全量轮数），不能用 `next.rounds.len()` 代替。
    #[serde(skip)]
    pub partial_round_count: usize,
}

/// 单轮思考明细
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceRound {
    /// 轮次编号（1-based，跨压缩累计）
    #[serde(default)]
    pub round: usize,
    /// 本轮模型可见文本（含工具轮里的思考说明）
    #[serde(default)]
    pub assistant_text: Option<String>,
    /// 本轮推理摘要（仅记录 provider 返回的可读 summary，密文永不落盘）
    #[serde(default)]
    pub reasoning: Vec<String>,
    /// 本轮工具调用及结果
    #[serde(default)]
    pub tool_calls: Vec<ThinkTraceToolCall>,
    /// 本轮输入 token
    #[serde(default)]
    pub input_tokens: u64,
    /// 本轮输出 token
    #[serde(default)]
    pub output_tokens: u64,
    /// 本轮提示缓存命中的输入 token（provider 未上报时为 0——不代表未命中）
    #[serde(default)]
    pub cached_tokens: u64,
    /// 本轮思考/推理消耗的输出 token（`output_tokens` 的子集；未上报时为 0）
    #[serde(default)]
    pub reasoning_tokens: u64,
    /// 本轮 LLM 调用耗时（毫秒）
    #[serde(default)]
    pub duration_ms: u64,
    /// 本轮的裁决结果：final / continue / reflect / exit
    #[serde(default)]
    pub decision: Option<String>,
    /// 本轮调用模型时实际下发的思考强度（0–100 分）；
    /// None = Provider 未启用该能力（请求体不下发）。按轮记录而非按 run，
    /// 未来支持逐轮动态调整时无需改存储结构。
    #[serde(default)]
    pub thinking_intensity: Option<ThinkingIntensity>,
}

/// run 内一次小脑调用的决策留痕
///
/// 小脑是可选增强：它的每次判断要么产出结论（进不进主流程、强度多少、
/// 哪些条目入选），要么降级走原逻辑。两种结果都必须能被事后追问，
/// 否则「小脑到底判了什么」只能靠翻日志。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThinkTraceCerebellum {
    /// 场景（`DecisionRequest.scene`：route / thinking_strength / memory_relevance / skill_relevance）
    #[serde(default)]
    pub scene: String,
    /// 决策结论（选中的选项或分数；`fallback` = 未产出可用结论）
    #[serde(default)]
    pub decision: String,
    /// 置信度（Pick 的概率；未产出结论时为 None）
    #[serde(default)]
    pub confidence: Option<f32>,
    /// 降级原因（None = 正常出结论）
    #[serde(default)]
    pub degraded: Option<String>,
    /// 是否真正影响了下游行为
    ///
    /// `false` 的典型场景：小脑算出了强度分数，但 Provider 未配置
    /// `thinking_intensity`（能力开关关闭），分数只留痕、请求体不下发。
    #[serde(default)]
    pub applied: bool,
    /// 本次判定的真实调用耗时（毫秒）；未发生调用时为 `None`
    ///
    /// 存在的唯一理由：**p95 时延的回收点**。时延只进日志的话，「这个场景到底
    /// 会不会恒超时」只能靠 grep 日志重算；进轨迹后可直接按 `scene` 分位统计。
    /// `applied=false`（放行）与 `degraded`（降级）也必须记 —— 它们同样付了
    /// 这次调用的时延，漏记会把 p95 系统性低估（只统计成功样本）。
    #[serde(default)]
    pub latency_ms: Option<u64>,
}

/// 单 run 最多保留多少条小脑留痕（防御：run 异常未收尾时不无限增长）
///
/// ## 为什么是 64（2026-11-10 由 32 上调）
///
/// 32 的隐含假设是「一个小脑写入点」，实际写入点有 **31 处 / 9 个场景**，且
/// **记账条目也占名额**：`empty_input`（intent_gate 放行）、`fallback` +
/// `budget_exhausted`、`circuit_open`（熔断开闸）、`capacity_too_small`、
/// `entry_gate` 的 `Skip` —— 它们不带业务结论，却与真结论抢同一批槽位。
/// 实测最坏口径（compaction 回环时同一 run 的记账条数）达 **58 条**，32 会稳定
/// 截断 26 条，且「哪些 scene 被丢」在轨迹里不可知（只有 `dropped` 总数）。
///
/// 64 的取值依据：覆盖 58 条最坏口径并留 6 条余量，同时仍是有界值 ——
/// 不存在「按 scene 分桶」那样的结构性改动（分桶要改 `record_cerebellum` 的
/// 键、`take_cerebellum` 的返回形状与查询侧合并口径，收益只是「丢得更有规律」，
/// 而 64 已让常规 run 不再触顶）。`dropped` 计数与收尾一次 `warn` 的契约不变：
/// 真撞顶时仍然是「可计数事实」，不沉默。
const CEREBELLUM_BUFFER_MAX_PER_RUN: usize = 64;

/// 单 run 的小脑留痕缓冲：条目 + 溢出丢弃计数
///
/// 为什么单独记 `dropped` 而不是只靠 `items.len()`：容量封顶后长度恒为
/// `CEREBELLUM_BUFFER_MAX_PER_RUN`，「丢了几条」在长度上一片空白 ——
/// 「轨迹看着完整、其实少了 26 条」是最难查的失败形态，所以丢弃必须是
/// **可计数事实**，不能沉默。
#[derive(Default)]
struct CerebellumBufferEntry {
    items: Vec<ThinkTraceCerebellum>,
    /// 因超出容量被丢弃的条数（同一 run 内累加）
    dropped: usize,
}

impl CerebellumBufferEntry {
    /// 追加一条：超容量只累加丢弃计数
    ///
    /// 这里**不打日志**：日志统一在 [`take_cerebellum`] 收尾处打一次。
    /// 两个理由：①每次丢弃打一行会变成日志风暴，换一个坑；
    /// ②丢弃发生时的计数只是瞬时值，不是本 run 的丢弃总量 ——
    /// 「丢了几条」要等收尾才数得准。
    fn push(&mut self, item: ThinkTraceCerebellum) {
        if self.items.len() < CEREBELLUM_BUFFER_MAX_PER_RUN {
            self.items.push(item);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    /// 取走全部条目与丢弃计数（清空自身，可重复调用）
    fn drain(&mut self) -> (Vec<ThinkTraceCerebellum>, usize) {
        let dropped = std::mem::take(&mut self.dropped);
        (std::mem::take(&mut self.items), dropped)
    }
}

/// 按 run 收集的小脑留痕缓冲
///
/// 为什么不层层透传 `Arc<Mutex<Vec<_>>>`：小脑调用点横跨 run 的四个阶段
/// （路由在循环之前、强度在入口、两路精选在检索节点），透传要穿过 4 个结构体；
/// 而轨迹是纯观测数据，丢失不影响业务。用有界缓冲换掉 4 处结构体扩张。
static CEREBELLUM_BUFFER: OnceLock<Mutex<HashMap<String, CerebellumBufferEntry>>> = OnceLock::new();

/// 追加一条小脑留痕（未初始化 / 空 run_id 时静默忽略，不产生副作用）
pub fn record_cerebellum(run_id: &str, item: ThinkTraceCerebellum) {
    if run_id.is_empty() || BASE_DATA_PATH.get().is_none() {
        return;
    }
    let buffer = CEREBELLUM_BUFFER.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut map) = buffer.lock() else {
        return;
    };
    // 防御：正常路径每个 run 都会在收尾时 take 走；若引擎异常中断导致残留，
    // 用总量上限兜住，避免进程内 map 无界增长。
    if map.len() > CEREBELLUM_BUFFER_MAX_PER_RUN * 8 {
        map.clear();
    }
    map.entry(run_id.to_string()).or_default().push(item);
}

/// 取走并清理某 run 的全部小脑留痕（run 收尾落盘时调用一次）
///
/// 本 run 若有溢出丢弃，这里**恰好打一次** `warn`，带丢弃条数与保留条数：
/// 收尾 `take` 每 run 只调用一次，且取走后该 run 的状态被整体移除，
/// 所以「每 run 至多一次告警」是结构保证，不依赖调用方自觉。
pub fn take_cerebellum(run_id: &str) -> Vec<ThinkTraceCerebellum> {
    let Some(buffer) = CEREBELLUM_BUFFER.get() else {
        return Vec::new();
    };
    let Ok(mut map) = buffer.lock() else {
        return Vec::new();
    };
    let Some(mut entry) = map.remove(run_id) else {
        return Vec::new();
    };
    let (items, dropped) = entry.drain();
    if dropped > 0 {
        log_warn!(
            "小脑轨迹缓冲已截断: run_id={}, dropped={}, kept={}, cap={}",
            run_id,
            dropped,
            items.len(),
            CEREBELLUM_BUFFER_MAX_PER_RUN
        );
    }
    items
}

/// 单轮内的一次工具调用
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceToolCall {
    /// 工具调用 trace ID（可据此查 tool_tracing 的完整入参/出参）
    #[serde(default)]
    pub call_id: Option<String>,
    /// 工具 wire name
    #[serde(default)]
    pub name: String,
    /// 入参 JSON 预览（截断）
    #[serde(default)]
    pub arguments: String,
    /// 结果预览（截断；未执行完成为 None）
    #[serde(default)]
    pub result: Option<String>,
    /// 执行状态：ok / error / blocked / not_found / pending
    #[serde(default)]
    pub status: String,
}

/// 单个节点的执行记录
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceOccurrence {
    /// 节点名（blueprint 中的 NodeDef）
    #[serde(default)]
    pub node: String,
    /// 节点执行序号（同节点重复执行时递增）
    #[serde(default)]
    pub seq: usize,
    /// 所属图步编号
    #[serde(default)]
    pub step: usize,
    /// 节点产出的准入信号（None 表示未产出）
    #[serde(default)]
    pub signal: Option<String>,
}

/// 一次选路记录
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkTraceRoute {
    /// 图步编号
    #[serde(default)]
    pub step: usize,
    /// 候选节点
    #[serde(default)]
    pub candidates: Vec<String>,
    /// 实际选择节点
    #[serde(default)]
    pub chosen: String,
    /// 选路来源（unique / deterministic / advisor / fallback）
    #[serde(default)]
    pub via: String,
    /// 选择原因（advisor / fallback 时通常有值）
    #[serde(default)]
    pub reason: Option<String>,
}

// ====================================================================
// 查询
// ====================================================================

/// Think trace 分页查询条件
///
/// `agent_id` 为 `None` 时遍历全部 Agent 目录；`project_id` / `task_id` 为 entry
/// 字段过滤，用于对话页按项目聚合多 Agent 轨迹。
///
/// `run_id` 是**精确匹配**：think trace 的 `run_id` 在 awaken / settle / compact
/// 场景下就是 MemoryTrace.id，因此记忆详情可凭自身 id 反查本次思考过程。
#[derive(Debug, Clone, Default)]
pub struct ThinkTraceQuery {
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    /// 精确匹配某次 run（即 MemoryTrace.id）
    pub run_id: Option<String>,
    pub scene: Option<String>,
    /// created_at 下界（含）
    pub started_after: Option<u64>,
    /// created_at 上界（含）
    pub started_before: Option<u64>,
    pub pagination: PaginationParams,
}

/// 查询 think trace（未初始化时返回空页，不产生文件系统副作用）
pub fn query(query: ThinkTraceQuery) -> anyhow::Result<PagedResult<ThinkTraceEntry>> {
    let Some(base) = BASE_DATA_PATH.get() else {
        return Ok(PagedResult::default());
    };
    query_at(base, &query)
}

/// 在指定基础目录下查询（包内可测，避免依赖全局初始化）
///
/// ## 成本模型：以及为什么这里不做早停
///
/// 扫描是**全量**的：每个 JSONL 分片逐行读 + 全量反序列化（[`read_matching_entries`]
/// 无早停），再合并、排序、分页。单次查询成本 ≈ 匹配行数 × 单行大小。
///
/// 逐轮增量落盘让行数从「1 行 / run」变成「N+1 行 / run」，但增量行只带**单轮**
/// `rounds`、收尾行才带全量，所以**解析的数据量约 2×** —— 不是行数那个 N+1 倍。
/// 这是「run 未结束也能查到半成品」的既定代价，量级可接受。
///
/// ⚠️ 不要为了省这点成本加「读够一页就停」的早停：`total` 必须是**合并后**的真实
/// 总数，前端 `has_next = offset + len < total` 与「共 N 条」都依赖它。早停会把
/// total 变成「已扫描到的条数」，翻页按钮当场失效（永远推不出下一页）。真要优化，
/// 正确方向是缩小时间窗口（调用方传 `started_after`，由 [`date_in_range`] 在文件级
/// 粗筛掉整段历史分片），而不是截断扫描过程。
pub(crate) fn query_at(
    base_data_path: &Path,
    query: &ThinkTraceQuery,
) -> anyhow::Result<PagedResult<ThinkTraceEntry>> {
    let limit = query
        .pagination
        .limit
        .unwrap_or(DEFAULT_THINK_TRACE_QUERY_LIMIT);
    if limit > MAX_THINK_TRACE_QUERY_LIMIT {
        anyhow::bail!(
            "think trace query limit exceeds maximum {}",
            MAX_THINK_TRACE_QUERY_LIMIT
        );
    }
    let offset = query.pagination.offset.unwrap_or(0);

    let mut entries = Vec::new();
    for agent_dir in agent_dirs(base_data_path, query.agent_id.as_deref())? {
        let trace_dir = agent_dir.join("think_trace");
        if !trace_dir.is_dir() {
            continue;
        }
        for path in jsonl_files(&trace_dir)? {
            if !date_in_range(&path, query.started_after, query.started_before) {
                continue;
            }
            read_matching_entries(&path, query, &mut entries)?;
        }
    }

    // 同一 run 内可能因新输入重跑思考循环（意图修订 / 执行回退），每次重跑都会
    // 追加一行。先按时间正序把同一 `(agent_id, run_id, scene)` 的多行合成一条，
    // 否则「同一动作被重新触发」会被展示成两个动作、各自统计轮次。
    entries.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.run_id.cmp(&b.run_id))
    });
    let mut entries = merge_same_run(entries);

    // 最新在前；同一毫秒内按 run_id 倒序，保证分页顺序稳定
    entries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });

    let total = entries.len();
    let items = entries.into_iter().skip(offset).take(limit).collect();
    Ok(PagedResult { items, total })
}

/// 合并同一 run 内被重新触发而追加的多行轨迹
///
/// `run_id` 标识一次图 run，但一次 run 里思考循环可能执行多次：
/// - 新消息触发意图修订时，`IntentNode` 重跑，`intent-{main}` 轨迹追加一行；
/// - `poll_exit` 回退执行时，Deliberate 的思考循环重跑，主 run 轨迹追加一行。
///
/// 这些行共享 `(agent_id, run_id, scene)`，是**同一个动作的不同片段**。查询层
/// 按该键聚合，让轨迹面板呈现一条连续路径，而不是两个各自计数的动作。
fn merge_same_run(entries: Vec<ThinkTraceEntry>) -> Vec<ThinkTraceEntry> {
    let mut merged: Vec<ThinkTraceEntry> = Vec::with_capacity(entries.len());
    // 键与前端展示标识同源（common::api::think_trace_entry_key）
    let mut positions: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let key = common::api::think_trace_entry_key(&entry.agent_id, &entry.run_id, &entry.scene);
        match positions.get(&key).copied() {
            Some(index) => merge_into(&mut merged[index], entry),
            None => {
                // 首行不经过 merge_into，若是增量行需在这里补上片段计数，
                // 否则该行不会被本片段的收尾行摘除（会多出一轮）
                let mut first = entry;
                if first.partial {
                    first.partial_round_count = first.rounds.len();
                }
                positions.insert(key, merged.len());
                merged.push(first);
            }
        }
    }
    merged
}

/// 把后一段轨迹并入前一段（入参按 `created_at` 正序，前一段即更早的片段）
///
/// - `created_at` / `project_id` / `task_id` 保留最早片段的值（run 起点语义，
///   与 `MemoryTrace.id` 的回查口径一致）
/// - `steps` / `advisor_calls` 求和
/// - `rounds` / `occurrences` / `routes` 按时间拼接，并让轮次、节点序号与图步号
///   连续：后一段若从 1 重新计数则整体后移，若已按唤醒级累计则原样保留
/// - `fingerprint` 由合并后的 occurrence 序列重算
fn merge_into(base: &mut ThinkTraceEntry, next: ThinkTraceEntry) {
    let step_offset = base.steps;

    // 收尾行（partial=false）到达：先摘掉「本片段」逐轮增量累积的 rounds，
    // 再按跨片段偏移拼接收尾行的完整 rounds。
    //
    // ⚠️ 只摘本片段，不能整体替换 `base.rounds`：一次唤醒内可能因意图修订多次
    // 重跑（见 [`merge_same_run`] 注释），每个片段各自 N 行增量 + 1 行收尾。
    // 早期版本「收尾行整体替换」会让第二个片段的收尾行把第一个片段的轮次
    // 一并冲掉；直接拼接又会让本片段每轮出现两次。
    // 增量行不带图执行信息，故 steps / occurrences / routes 的累加不受影响。
    let final_line = !next.partial;
    let next_round_count = next.rounds.len();
    if final_line {
        let drop_count = base.partial_round_count.min(base.rounds.len());
        base.rounds.truncate(base.rounds.len() - drop_count);
        base.partial_round_count = 0;
    }

    // 轮次：意图重跑从 1 计数，需要后移；执行回退已带 `start_round`（累计值），
    // 已是连续编号，不能再加偏移
    let round_offset = match (base.rounds.last(), next.rounds.first()) {
        (Some(last), Some(first)) if first.round <= last.round => {
            last.round.saturating_add(1).saturating_sub(first.round)
        }
        _ => 0,
    };
    for mut round in next.rounds {
        round.round = round.round.saturating_add(round_offset);
        base.rounds.push(round);
    }
    // 增量行可能部分写失败（只告警），按实际收到的行数累计，供本片段收尾行摘除
    if !final_line {
        base.partial_round_count = base.partial_round_count.saturating_add(next_round_count);
    }

    // occurrence：节点级序号跨片段续编号，图步号整体后移。
    // 偏移必须取自合并前的快照：同一片段内后面的 occurrence 不能再吃一遍偏移。
    let base_seq_by_node: HashMap<String, usize> = base
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.node.clone(), occurrence.seq))
        .collect();
    for mut occurrence in next.occurrences {
        if let Some(previous) = base_seq_by_node.get(&occurrence.node).copied() {
            occurrence.seq = occurrence.seq.saturating_add(previous);
        }
        occurrence.step = occurrence.step.saturating_add(step_offset);
        base.occurrences.push(occurrence);
    }

    for mut route in next.routes {
        route.step = route.step.saturating_add(step_offset);
        base.routes.push(route);
    }

    // 小脑留痕：收尾行带 run 级全量，chain_reply 补丁行在其后再补一批 ——
    // 两者是同一 run 的两段留痕，按时间拼接。
    //
    // 不需要去重：`take_cerebellum` 是「取走即清空」，同一 run 的每个批次只会
    // 落进一行，拼接不会产生重复条目。若将来出现「多次 take + 多行补丁」，
    // 这里才是需要加去重的地方（当前没有这个路径）。
    base.cerebellum.extend(next.cerebellum);

    base.steps = base.steps.saturating_add(next.steps);
    base.advisor_calls = base.advisor_calls.saturating_add(next.advisor_calls);
    // 指纹是实际 occurrence 序列的压缩表达，合并后必须重算
    base.fingerprint = base
        .occurrences
        .iter()
        .map(|occurrence| format!("{}#{}", occurrence.node, occurrence.seq))
        .collect::<Vec<_>>()
        .join(">");

    if base.project_id.is_none() {
        base.project_id = next.project_id;
    }
    if base.task_id.is_none() {
        base.task_id = next.task_id;
    }
    // 本行粒度跟随最新写入的那行：收尾行之后不再是「半成品」
    base.partial = next.partial;
}

/// 列出待扫描的 Agent 数据目录
fn agent_dirs(base_data_path: &Path, agent_id: Option<&str>) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(agent_id) = agent_id {
        return Ok(vec![paths::agent_data_dir(base_data_path, agent_id)]);
    }
    let root = paths::agents_root_dir(base_data_path);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn jsonl_files(trace_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(trace_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// 按日期文件名做粗筛（避免为时间窗口扫描全部历史分片）
fn date_in_range(path: &Path, started_after: Option<u64>, started_before: Option<u64>) -> bool {
    let Some(date) = path.file_stem().and_then(|s| s.to_str()) else {
        return true;
    };
    if let Some(after) = started_after.and_then(date_string)
        && date < after.as_str()
    {
        return false;
    }
    if let Some(before) = started_before.and_then(date_string)
        && date > before.as_str()
    {
        return false;
    }
    true
}

/// 毫秒时间戳 → 落盘文件名使用的本地日期（YYYYMMDD）
fn date_string(ms: u64) -> Option<String> {
    let millis = i64::try_from(ms).ok()?;
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y%m%d")
            .to_string()
    })
}

fn read_matching_entries(
    path: &Path,
    query: &ThinkTraceQuery,
    entries: &mut Vec<ThinkTraceEntry>,
) -> anyhow::Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        // 单行损坏（并发写入残留等）只跳过该行，不让整次查询失败
        let entry: ThinkTraceEntry = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(err) => {
                log_warn!(
                    "跳过无法解析的思考轨迹行: file={}, err={}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        if matches_query(&entry, query) {
            entries.push(entry);
        }
    }
    Ok(())
}

fn matches_query(entry: &ThinkTraceEntry, query: &ThinkTraceQuery) -> bool {
    query
        .agent_id
        .as_ref()
        .is_none_or(|agent_id| entry.agent_id == *agent_id)
        && query
            .project_id
            .as_ref()
            .is_none_or(|project_id| entry.project_id.as_deref() == Some(project_id.as_str()))
        && query
            .task_id
            .as_ref()
            .is_none_or(|task_id| entry.task_id.as_deref() == Some(task_id.as_str()))
        && query
            .run_id
            .as_ref()
            .is_none_or(|run_id| entry.run_id == *run_id)
        && query
            .scene
            .as_ref()
            .is_none_or(|scene| entry.scene == *scene)
        && query
            .started_after
            .is_none_or(|started_after| entry.created_at >= started_after)
        && query
            .started_before
            .is_none_or(|started_before| entry.created_at <= started_before)
}

#[cfg(test)]
#[path = "think_trace_tests.rs"]
mod tests;
