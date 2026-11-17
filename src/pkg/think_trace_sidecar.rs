//! 思考轨迹的旁车索引（业务适配层）
//!
//! 机械部分下沉在 [`crate::pkg::jsonl::sidecar::JsonlSidecar`]。本模块定义 think 专属
//! 过滤字段与匹配逻辑。目录 `agents/{agent_id}/think_trace/{date}.jsonl` 按 agent 隔离。
//!
//! ⚠️ 与 tool/memory 不同：think 查询必须在**所有命中行**上先 `merge_same_run` 再分页
//! （`total` 是合并后的真实总数，前端翻页依赖它）。因此这里**不截断**命中数——返回
//! 全部命中坐标，由调用方 seek 读回后走原有 merge/sort/pagination。索引的收益来自
//! 「过滤阶段只读轻量索引、只反序列化命中行」，而不是减少命中行数。

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::pkg::jsonl::sidecar::JsonlSidecar;
use crate::pkg::jsonl::writer::DailyJsonlWriter;

use super::{ThinkTraceEntry, ThinkTraceQuery};

/// 索引过滤字段（不含 rounds/reasoning 这类大载荷）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkIndexFields {
    agent_id: String,
    run_id: String,
    project_id: Option<String>,
    task_id: Option<String>,
    scene: String,
    created_at: u64,
}

impl ThinkIndexFields {
    fn from_entry(entry: &ThinkTraceEntry) -> Self {
        Self {
            agent_id: entry.agent_id.clone(),
            run_id: entry.run_id.clone(),
            project_id: entry.project_id.clone(),
            task_id: entry.task_id.clone(),
            scene: entry.scene.clone(),
            created_at: entry.created_at,
        }
    }

    /// 增量补齐时从原文行字节提取；入口可能传的是 `serde_json::Value`（写入侧入参），
    /// 这里统一按 `ThinkTraceEntry` 解析——落盘行字段与它一一对应（见 think_trace::append）。
    fn from_line(line: &[u8], _line_number: u64) -> Option<Self> {
        serde_json::from_slice::<ThinkTraceEntry>(line)
            .ok()
            .map(|entry| Self::from_entry(&entry))
    }
}

#[derive(Debug, Clone)]
pub(super) struct ThinkSidecar {
    inner: JsonlSidecar<ThinkIndexFields>,
}

impl ThinkSidecar {
    pub fn new(trace_dir: std::path::PathBuf) -> Self {
        Self {
            inner: JsonlSidecar::new(trace_dir),
        }
    }

    /// 写时维护：原文已写、调用方持锁的临界区内追加索引。
    /// 写入侧入参是 `serde_json::Value`，按字段提取即可（无需完整 ThinkTraceEntry）
    pub fn append_record(
        &self,
        date: &str,
        entry: &serde_json::Value,
        offset: u64,
        len: u64,
    ) -> Result<()> {
        let fields = ThinkIndexFields {
            agent_id: get_str(entry, "agent_id").unwrap_or_default(),
            run_id: get_str(entry, "run_id").unwrap_or_default(),
            project_id: get_opt_str(entry, "project_id"),
            task_id: get_opt_str(entry, "task_id"),
            scene: get_str(entry, "scene").unwrap_or_default(),
            created_at: entry
                .get("created_at")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        };
        self.inner.append_record(date, offset, len, fields)
    }

    /// 查询：返回全部命中坐标（不截断，调用方 merge 后再分页）
    pub fn query(&self, query: &ThinkTraceQuery) -> Result<Vec<ThinkHit>> {
        let mut hits = Vec::new();
        for date in self.inner.data_dates()? {
            // 文件级日期粗筛（与旧 date_in_range 同口径）
            if !date_in_range(&date, query.started_after, query.started_before) {
                continue;
            }
            let inner = &self.inner;
            let parse = ThinkIndexFields::from_line;
            // 恢复 + 读取绑在同一把锁内：杜绝并发查询时一个重建清空索引、另一个读到半截。
            let records = DailyJsonlWriter::with_append_lock(|| inner.records_fresh(&date, parse))?;
            for record in records {
                if matches_index(&record.fields, query) {
                    hits.push(ThinkHit {
                        date: date.clone(),
                        offset: record.offset,
                        len: record.len,
                    });
                }
            }
        }
        Ok(hits)
    }

    /// seek 读回一条命中 entry
    pub fn read_entry(&self, date: &str, offset: u64, len: u64) -> Result<ThinkTraceEntry> {
        self.inner.read_line_at(date, offset, len)
    }
}

#[derive(Debug, Clone)]
pub(super) struct ThinkHit {
    pub date: String,
    pub offset: u64,
    pub len: u64,
}

fn get_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn get_opt_str(value: &serde_json::Value, key: &str) -> Option<String> {
    match value.get(key) {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => v.as_str().map(str::to_string),
    }
}

/// 按日期文件名粗筛（毫秒时间戳 → 本地日期）
fn date_in_range(date: &str, after: Option<u64>, before: Option<u64>) -> bool {
    if let Some(after) = after.and_then(date_string)
        && date < after.as_str()
    {
        return false;
    }
    if let Some(before) = before.and_then(date_string)
        && date > before.as_str()
    {
        return false;
    }
    true
}

fn date_string(ms: u64) -> Option<String> {
    let millis = i64::try_from(ms).ok()?;
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y%m%d")
            .to_string()
    })
}

/// 索引层过滤：字段与 `matches_query` 一一对应
fn matches_index(fields: &ThinkIndexFields, query: &ThinkTraceQuery) -> bool {
    query
        .agent_id
        .as_ref()
        .is_none_or(|agent_id| fields.agent_id == *agent_id)
        && query
            .agent_ids
            .as_ref()
            .is_none_or(|agent_ids| agent_ids.iter().any(|id| id == &fields.agent_id))
        && query
            .project_id
            .as_ref()
            .is_none_or(|project_id| fields.project_id.as_deref() == Some(project_id.as_str()))
        && query
            .task_id
            .as_ref()
            .is_none_or(|task_id| fields.task_id.as_deref() == Some(task_id.as_str()))
        && query
            .run_id
            .as_ref()
            .is_none_or(|run_id| fields.run_id == *run_id)
        && query
            .scene
            .as_ref()
            .is_none_or(|scene| fields.scene == *scene)
        && query
            .started_after
            .is_none_or(|started_after| fields.created_at >= started_after)
        && query
            .started_before
            .is_none_or(|started_before| fields.created_at <= started_before)
}
