//! 记忆轨迹的旁车索引（业务适配层）
//!
//! 机械部分下沉在 [`crate::pkg::jsonl::sidecar::JsonlSidecar`]。本模块定义 memory
//! 专属的过滤字段与匹配逻辑。目录布局是 `agents/{agent_id}/memory/{date}.jsonl`
//! （每 agent 独立目录），因此 `agent_id` 不进索引字段——目录已经隔离。
//!
//! `line_number` 随索引记录落盘：这样 `resolve_trace_positions_at`（知识引用链）也能
//! 走索引点查，不再逐行扫原文。引用链的物理坐标语义保持不变（下一阶段才切 offset）。

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::models::memory::MemoryTrace;
use crate::pkg::jsonl::sidecar::JsonlSidecar;
use crate::pkg::jsonl::writer::DailyJsonlWriter;
use crate::service::dao::memory::MemoryTraceQuery;

/// 索引过滤字段（不含 input/output 大原文）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryIndexFields {
    id: String,
    task_id: Option<String>,
    log_id: String,
    created_at: i64,
    line_number: u64,
}

impl MemoryIndexFields {
    fn from_trace(trace: &MemoryTrace, line_number: u64) -> Self {
        Self {
            id: trace.id.clone(),
            task_id: trace.task_id.clone(),
            log_id: trace.log_id.clone(),
            created_at: trace.created_at,
            line_number,
        }
    }

    /// 增量补齐时从原文行字节提取，物理行号一并落盘（引用链坐标）；
    /// 坏行返回 None（底层跳过），但行号按物理行递增不受影响。
    fn from_line(line: &[u8], line_number: u64) -> Option<Self> {
        serde_json::from_slice::<MemoryTrace>(line)
            .ok()
            .map(|trace| Self::from_trace(&trace, line_number))
    }
}

#[derive(Debug, Clone)]
pub(super) struct TraceSidecar {
    inner: JsonlSidecar<MemoryIndexFields>,
}

impl TraceSidecar {
    pub fn new(trace_dir: std::path::PathBuf) -> Self {
        Self {
            inner: JsonlSidecar::new(trace_dir),
        }
    }

    /// 写时维护：原文已写、调用方持锁的临界区内追加索引（line_number 随记录落盘）
    pub fn append_record(
        &self,
        date: &str,
        trace: &MemoryTrace,
        line_number: usize,
        offset: u64,
        len: u64,
    ) -> Result<()> {
        self.inner.append_record(
            date,
            offset,
            len,
            MemoryIndexFields::from_trace(trace, line_number as u64),
        )
    }

    /// 查询：返回命中坐标（调用方 seek 原文读完整 trace）
    pub fn query(&self, query: &MemoryTraceQuery) -> Result<Vec<MemoryHit>> {
        let mut hits = Vec::new();
        for date in self.inner.data_dates()? {
            // 文件级日期粗筛（与旧实现的 date_in_range 同口径）
            if !date_in_range(&date, query) {
                continue;
            }
            let inner = &self.inner;
            let parse = MemoryIndexFields::from_line;
            // 恢复 + 读取绑在同一把锁内：杜绝并发查询时一个重建清空索引、另一个读到半截。
            let records = DailyJsonlWriter::with_append_lock(|| inner.records_fresh(&date, parse))?;
            for record in records {
                if matches_index(&record.fields, query) {
                    hits.push(MemoryHit {
                        date: date.clone(),
                        offset: record.offset,
                        len: record.len,
                    });
                }
            }
        }
        Ok(hits)
    }

    /// 按 trace_id 精确解析物理位置（引用链用）：索引命中即返回，不扫原文。
    ///
    /// 用 HashSet 承载待查 id 并逐个 remove，全部命中即提前收工；不再是每条索引行
    /// 对切片做 O(n) 扫描的 O(索引×查询数) 复杂度。
    pub fn resolve_positions(&self, trace_ids: &[&str]) -> Result<Vec<PositionHit>> {
        let mut wanted: std::collections::HashSet<&str> = trace_ids.iter().copied().collect();
        let mut found = Vec::new();
        if wanted.is_empty() {
            return Ok(found);
        }
        for date in self.inner.data_dates()? {
            let inner = &self.inner;
            let parse = MemoryIndexFields::from_line;
            let records = DailyJsonlWriter::with_append_lock(|| inner.records_fresh(&date, parse))?;
            for record in records {
                if wanted.remove(record.fields.id.as_str()) {
                    found.push(PositionHit {
                        trace_id: record.fields.id,
                        date_filename: format!("{date}.jsonl"),
                        line_number: record.fields.line_number,
                    });
                }
            }
            if wanted.is_empty() {
                break;
            }
        }
        Ok(found)
    }

    /// 增量补齐时读回完整 trace（query 命中后由调用方 seek）
    pub fn read_trace(&self, date: &str, offset: u64, len: u64) -> Result<MemoryTrace> {
        self.inner.read_line_at(date, offset, len)
    }
}

#[derive(Debug, Clone)]
pub(super) struct MemoryHit {
    pub date: String,
    pub offset: u64,
    pub len: u64,
}

/// 引用链位置命中（由 trace.rs 转成 `MemoryTracePosition`）
#[derive(Debug, Clone)]
pub(super) struct PositionHit {
    pub trace_id: String,
    pub date_filename: String,
    pub line_number: u64,
}

/// 按日期文件名粗筛（与旧 `date_in_range` 同口径，避免为时间窗口扫全部历史分片）
fn date_in_range(date: &str, query: &MemoryTraceQuery) -> bool {
    if let Some(after) = query.created_after.and_then(date_string)
        && date < after.as_str()
    {
        return false;
    }
    if let Some(before) = query.created_before.and_then(date_string)
        && date > before.as_str()
    {
        return false;
    }
    true
}

/// 秒级时间戳 → 本地日期（YYYYMMDD）
fn date_string(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y%m%d")
            .to_string()
    })
}

/// 索引层过滤：字段与 `matches_trace_query` / `MemoryTraceQuery` 一一对应
fn matches_index(fields: &MemoryIndexFields, query: &MemoryTraceQuery) -> bool {
    query
        .task_id
        .as_deref()
        .is_none_or(|task_id| fields.task_id.as_deref() == Some(task_id))
        && query
            .log_id
            .as_deref()
            .is_none_or(|log_id| fields.log_id == log_id)
        && query
            .trace_id
            .as_deref()
            .is_none_or(|trace_id| fields.id == trace_id)
        && query
            .created_after
            .is_none_or(|after| fields.created_at >= after)
        && query
            .created_before
            .is_none_or(|before| fields.created_at <= before)
}
