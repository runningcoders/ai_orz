//! 工具调用 trace 的旁车索引（业务适配层）
//!
//! 机械部分（分片发现 / watermark 校验 / 增量补齐 / 坏行重建 / seek 读回）已下沉到
//! 通用底层 [`crate::pkg::jsonl::sidecar::JsonlSidecar`]。本模块只负责：
//! - 定义工具调用专属的过滤字段 [`CallIndexFields`]（不含 input/output 大原文）；
//! - 用这些字段实现与 `ToolCallQuery` 一一对应的过滤；
//! - 编排「确保新鲜 → 索引层过滤/排序/截断 → 返回命中坐标」。

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallQuery;
use crate::pkg::jsonl::sidecar::JsonlSidecar;
use crate::pkg::jsonl::writer::DailyJsonlWriter;

/// 索引里的过滤字段：查询所需的最小集合，不含完整 input/output
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CallIndexFields {
    call_id: String,
    tool_id: String,
    agent_id: Option<String>,
    task_id: Option<String>,
    project_id: Option<String>,
    status: ToolCallStatus,
    started_at: u64,
    finished_at: u64,
}

impl CallIndexFields {
    fn from_entry(entry: &ToolCallEntry) -> Self {
        Self {
            call_id: entry.call_id.clone(),
            tool_id: entry.tool_id.clone(),
            agent_id: entry.agent_id.clone(),
            task_id: entry.task_id.clone(),
            project_id: entry.project_id.clone(),
            status: entry.status,
            started_at: entry.started_at,
            finished_at: entry.finished_at,
        }
    }

    /// 增量补齐时从原文行字节提取过滤字段；坏行返回 None（底层跳过）
    fn from_line(line: &[u8], _line_number: u64) -> Option<Self> {
        serde_json::from_slice::<ToolCallEntry>(line)
            .ok()
            .map(|entry| Self::from_entry(&entry))
    }
}

/// 命中结果：所属日期 + 原文字节坐标（供调用方 seek 读回完整 entry）
#[derive(Debug, Clone)]
pub(super) struct Hit {
    pub date: String,
    pub offset: u64,
    pub len: u64,
}

/// 旁车索引句柄（持有 trace 目录，无全局状态，便于临时目录隔离测试）
#[derive(Debug, Clone)]
pub(super) struct SidecarIndex {
    inner: JsonlSidecar<CallIndexFields>,
}

impl SidecarIndex {
    pub fn new(trace_dir: std::path::PathBuf) -> Self {
        Self {
            inner: JsonlSidecar::new(trace_dir),
        }
    }

    /// 写时维护：原文已写、调用方持锁的临界区内追加索引（失败只告警由调用方处理）
    pub fn append_record(
        &self,
        date: &str,
        entry: &ToolCallEntry,
        offset: u64,
        len: u64,
    ) -> Result<()> {
        self.inner
            .append_record(date, offset, len, CallIndexFields::from_entry(entry))
    }

    /// 查询：对每个原文分片确保索引新鲜，在索引层过滤/排序，返回全部命中坐标。
    ///
    /// ⚠️ 这里**不截断**：截断下移到 [`logger::query_calls`](super::logger::ToolCallLogger::query_calls)
    /// 回原文之后——索引命中若因原文损坏读回失败，需要靠后续候选补位；在索引层先截断会
    /// 让坏行吃掉 limit 配额，返回条数少于 limit（与旧文件扫描「跳过坏行再截断」不等价）。
    pub fn query(&self, query: &ToolCallQuery) -> Result<Vec<Hit>> {
        let limit = query.limit.unwrap_or(1);
        if limit > super::logger::MAX_TOOL_CALL_QUERY_LIMIT {
            anyhow::bail!(
                "tool call query limit exceeds maximum {}",
                super::logger::MAX_TOOL_CALL_QUERY_LIMIT
            );
        }

        let mut hits: Vec<(CallIndexFields, String, u64, u64)> = Vec::new();
        for date in self.inner.data_dates()? {
            let inner = &self.inner;
            let parse = CallIndexFields::from_line;
            // 恢复 + 读取绑在同一把锁内：杜绝并发查询时一个重建清空索引、另一个读到半截。
            let records = DailyJsonlWriter::with_append_lock(|| inner.records_fresh(&date, parse))?;
            for record in records {
                if matches_index(&record.fields, query) {
                    hits.push((record.fields, date.clone(), record.offset, record.len));
                }
            }
        }

        hits.sort_by(|(a, _, _, _), (b, _, _, _)| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.finished_at.cmp(&a.finished_at))
                .then_with(|| b.call_id.cmp(&a.call_id))
        });

        Ok(hits
            .into_iter()
            .map(|(_, date, offset, len)| Hit { date, offset, len })
            .collect())
    }
}

/// 索引层过滤：字段与 `ToolCallQuery` 一一对应，用索引字段代替完整 entry
fn matches_index(fields: &CallIndexFields, query: &ToolCallQuery) -> bool {
    query
        .call_id
        .as_ref()
        .is_none_or(|call_id| fields.call_id == *call_id)
        && query
            .agent_id
            .as_ref()
            .is_none_or(|agent_id| fields.agent_id.as_deref() == Some(agent_id.as_str()))
        && query.agent_ids.as_ref().is_none_or(|agent_ids| {
            fields
                .agent_id
                .as_deref()
                .is_some_and(|agent_id| agent_ids.iter().any(|id| id == agent_id))
        })
        && query
            .project_id
            .as_ref()
            .is_none_or(|project_id| fields.project_id.as_deref() == Some(project_id.as_str()))
        && (!query.without_project || fields.project_id.is_none())
        && query
            .task_id
            .as_ref()
            .is_none_or(|task_id| fields.task_id.as_deref() == Some(task_id.as_str()))
        && query
            .tool_id
            .as_ref()
            .is_none_or(|tool_id| fields.tool_id == *tool_id)
        && query.status.is_none_or(|status| fields.status == status)
        && query
            .started_after
            .is_none_or(|started_after| fields.started_at >= started_after)
        && query
            .started_before
            .is_none_or(|started_before| fields.started_at <= started_before)
}
