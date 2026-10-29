//! 轮级思考图轨迹（`think_path` run trace）的落盘出口与查询入口
//!
//! 与 `tool_tracing` 同一套「显式初始化 + 未初始化即 no-op」模式：
//! 生产环境在 [`crate::pkg::init_all`] 注入 base_data_path；单元测试不初始化，
//! 因此不会往 `~/.ai_orz` 写文件（测试隔离红线）。
//!
//! 每次思考循环一行 JSONL（`agents/{agent_id}/think_trace/{YYYYMMDD}.jsonl`），
//! 记录节点 occurrence 序列与每次选路的候选/来源，供动态路由排障与回放。
//!
//! 查询侧按 **agent 维度** 分页：`agent_id` 缺省时遍历 `agents/*/think_trace`，
//! 供对话页「多 Agent 聚合流」使用；`project_id` / `task_id` 是 entry 字段过滤
//! （2026-10-22 起新写入的轨迹才有值，历史轨迹该字段为空）。

use crate::pkg::daily_jsonl::DailyJsonlWriter;
use crate::pkg::paths;
use common::api::{PagedResult, PaginationParams};
use common::enums::ThinkingIntensity;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
