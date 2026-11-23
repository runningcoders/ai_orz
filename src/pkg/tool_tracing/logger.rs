//! Tool call logging implementation using daily JSONL files
//!
//! Stores tool call traces at: {base_data_path}/tools/call_trace/{YYYYMMDD}.jsonl
//! Each line is a single ToolCallEntry with full input/output metadata
//!
//! 【边界决策 2026-09-19】**不按 `tool_id` 再分一层目录**（详见
//! [`crate::pkg::paths::tool_call_trace_dir`]）：`call_id` 是一次调用的唯一身份，
//! `tool_id` 只是 entry 的字段。拉平后未带 `tool_id` 的查询不再退化成全量目录枚举。

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use once_cell::sync::OnceCell;

use super::entry::{ToolCallEntry, ToolCallStatus};
use crate::pkg::jsonl::writer::DailyJsonlWriter;
use crate::pkg::paths;

pub const MAX_TOOL_CALL_QUERY_LIMIT: usize = 100;

/// Query filters for tool call trace entries.
#[derive(Debug, Clone, Default)]
pub struct ToolCallQuery {
    pub call_id: Option<String>,
    pub agent_id: Option<String>,
    /// Agent 集合过滤（对话页「当前任务出现过的 Agent」聚合视角）；
    /// `agent_id` 与 `agent_ids` 同时存在时两者都生效（交集）
    pub agent_ids: Option<Vec<String>>,
    pub project_id: Option<String>,
    /// 仅未归属项目的记录（默认对话右侧「工具」Tab 的全量游离视角）
    pub without_project: bool,
    pub task_id: Option<String>,
    pub tool_id: Option<String>,
    pub status: Option<ToolCallStatus>,
    pub started_after: Option<u64>,
    pub started_before: Option<u64>,
    /// Default: latest one matching entry.
    pub limit: Option<usize>,
    /// 跳过的命中条数（分页游标），默认 0
    pub offset: usize,
}

/// 一页工具调用记录 + 过滤条件命中的总条数
#[derive(Debug, Clone)]
pub struct ToolCallPage {
    pub items: Vec<ToolCallEntry>,
    pub total: usize,
}

/// Global ToolCallLogger singleton
static INSTANCE: OnceCell<ToolCallLogger> = OnceCell::new();

/// ToolCallLogger is a factory that provides daily JSONL writers for tool call tracing
///
/// This is a singleton - initialize once with base data path at application startup,
/// then get the global instance anywhere with `ToolCallLogger::get()`.
#[derive(Debug, Clone)]
pub struct ToolCallLogger {
    base_data_path: PathBuf,
}

impl ToolCallLogger {
    /// Initialize the global ToolCallLogger singleton
    /// Must be called once at application startup
    pub fn init(base_data_path: PathBuf) {
        let _ = INSTANCE.get_or_init(|| Self { base_data_path });
    }

    /// Get the global ToolCallLogger singleton instance
    /// Panics if not initialized yet
    pub fn get() -> &'static Self {
        INSTANCE.get().expect("ToolCallLogger not initialized")
    }

    /// 非 panic 版取单例：未初始化返回 `None`
    ///
    /// 供启动期「尽力而为」链路使用（如内置工具就绪自愈的留痕与 7 天去重）——
    /// 这些链路失败只应降级（跳过留痕/去重），绝不因为日志器未就绪而中断启动。
    pub fn try_get() -> Option<&'static Self> {
        INSTANCE.get()
    }

    /// Create a new ToolCallLogger instance (for direct use, prefer singleton)
    #[allow(dead_code)]
    pub fn new(base_data_path: PathBuf) -> Self {
        Self { base_data_path }
    }

    /// Get the writer for the shared call trace directory
    pub fn writer(&self) -> DailyJsonlWriter {
        DailyJsonlWriter::new(self.trace_dir())
    }

    /// Log a tool call entry to the daily JSONL file and its sidecar index.
    ///
    /// `tool_id` 仅作为 entry 字段落盘，不参与路径（见模块级边界决策）。
    ///
    /// 【写时维护索引】原文 append 与索引 append 必须共成败边界：原文写入成功后
    /// 立即在**同一把 `APPEND_LOCK` 临界区**内追加索引行（offset 由写入前的长度
    /// 得到，严格对应本行位置）。索引写失败只告警、不传播——原文已落盘，索引
    /// watermark 至多落后，读路径会增量补齐（见 `sidecar_index::ensure_fresh`）。
    pub fn log_call(&self, entry: ToolCallEntry) -> Result<()> {
        let writer = self.writer();
        let sidecar = self.sidecar_index();
        let call_id = entry.call_id.clone();
        let entry_ref = &entry;
        // 原文 append + 索引 append 在 append_with_index 内共用同一把 APPEND_LOCK，
        // 保证索引行顺序与原文行顺序严格一致；索引写失败只告警、不传播。
        writer.append_with_index(entry_ref, |date, offset, len| {
            if let Err(err) = sidecar.append_record(date, entry_ref, offset, len) {
                log_warn!(
                    "工具调用 trace 索引追加失败，将在下次查询补齐: date={}, call_id={}, err={}",
                    date,
                    call_id,
                    err
                );
            }
            Ok(())
        })?;
        Ok(())
    }

    fn sidecar_index(&self) -> super::sidecar_index::SidecarIndex {
        super::sidecar_index::SidecarIndex::new(self.trace_dir())
    }

    /// Read a logged tool call entry by date and line number
    #[allow(dead_code)]
    pub fn read_call(&self, date: &str, line_number: usize) -> Result<ToolCallEntry> {
        let writer = self.writer();
        writer.read_line_json(date, line_number)
    }

    /// Read a logged tool call entry by call ID.
    ///
    /// `call_id` 即一次调用的唯一身份，**不按 `tool_id` 收窄**：收窄会让
    /// 「同一个 call_id 落在别的工具名下」的历史行查不到，正是这次 UI 串号的成因之一。
    pub fn read_call_by_id(&self, call_id: &str) -> Result<Option<ToolCallEntry>> {
        let entries = self.query_calls(ToolCallQuery {
            call_id: Some(call_id.to_string()),
            limit: Some(1),
            ..Default::default()
        })?;
        Ok(entries.into_iter().next())
    }

    /// Query logged tool call entries by common fields.
    ///
    /// 【索引路径】先在旁车索引上过滤/排序/截断，再按字节坐标 seek 回原文读命中行。
    /// 索引缺失或落后时由 `sidecar_index` 增量重建（这也是历史无索引文件的兼容路径），
    /// 因此本方法的语义与文件扫描实现完全一致，只是不再全量反序列化 input/output。
    pub fn query_calls(&self, query: ToolCallQuery) -> Result<Vec<ToolCallEntry>> {
        Ok(self.query_calls_page(query)?.items)
    }

    /// 分页查询：返回当前 offset/limit 窗口内的条目，以及命中过滤条件的总条数。
    ///
    /// 总条数取自旁车索引命中数，不额外回读原文；分页在排序后的命中序列上
    /// skip(offset).take(limit)，与「最新优先」的全局排序一致。
    pub fn query_calls_page(&self, query: ToolCallQuery) -> Result<ToolCallPage> {
        let limit = query.limit.unwrap_or(1);
        let hits = self.sidecar_index().query(&query)?;
        let total = hits.len();
        if limit == 0 {
            return Ok(ToolCallPage {
                items: Vec::new(),
                total,
            });
        }
        let mut entries = Vec::with_capacity(hits.len().saturating_sub(query.offset).min(limit));
        for hit in hits.into_iter().skip(query.offset) {
            match self.read_at_offset(&hit.date, hit.offset, hit.len) {
                Ok(entry) => entries.push(entry),
                // 索引指向的行损坏/被清理：跳过并告警，不让一次坏行拖垮整次查询。
                // 它与文件扫描版「单行损坏跳过」的容错口径一致。
                Err(err) => log_warn!(
                    "跳过无法读取的工具调用轨迹行: date={}, offset={}, err={}",
                    hit.date,
                    hit.offset,
                    err
                ),
            }
            // 回读成功后才计入配额：坏行命中被跳过后由后续候选补位，返回满 limit 条。
            if entries.len() >= limit {
                break;
            }
        }
        Ok(ToolCallPage {
            items: entries,
            total,
        })
    }

    /// 按字节坐标精确读取一行并反序列化（O(命中行载荷)，不扫整文件）
    fn read_at_offset(&self, date: &str, offset: u64, len: u64) -> Result<ToolCallEntry> {
        use std::io::{Read, Seek, SeekFrom};

        let path = self.trace_dir().join(format!("{date}.jsonl"));
        let mut file = fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        let line = std::str::from_utf8(&buf)?.trim_end_matches('\n');
        Ok(serde_json::from_str(line)?)
    }

    fn trace_dir(&self) -> PathBuf {
        paths::tool_call_trace_dir(&self.base_data_path)
    }
}
