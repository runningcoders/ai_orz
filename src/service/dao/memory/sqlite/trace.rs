//! 记忆轨迹 DAO：写入每日 JSONL + 按条件扫描查询

use super::MemoryDaoSqliteImpl;
use crate::config;
use crate::models::memory::{MemoryTrace, MemoryTracePosition};
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::service::dao::memory::MemoryTraceQuery;
use common::api::{PagedResult, PaginationParams};
use common::error::Result;
use serde_json;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

// ==================== MemoryTrace 文件读取 ====================

/// 在指定基础目录下查询记忆轨迹（包内可测，避免依赖全局配置）
pub(crate) fn query_traces_at(
    base_data_path: &Path,
    query: &MemoryTraceQuery,
) -> Result<PagedResult<MemoryTrace>> {
    let limit = query
        .pagination
        .limit
        .unwrap_or(super::DEFAULT_MEMORY_TRACE_QUERY_LIMIT);
    if limit > super::MAX_MEMORY_TRACE_QUERY_LIMIT {
        return Err(common::error::Error::bad_request(format!(
            "memory trace query limit must be <= {}",
            super::MAX_MEMORY_TRACE_QUERY_LIMIT
        )));
    }
    let offset = query.pagination.offset.unwrap_or(0);

    let mut traces = Vec::new();
    for dir in memory_trace_dirs(base_data_path, query.agent_id.as_deref())? {
        for path in jsonl_files(&dir)? {
            if !date_in_range(&path, query.created_after, query.created_before) {
                continue;
            }
            read_matching_traces(&path, query, &mut traces);
        }
    }

    // 最新在前；同一秒内按 id 倒序，保证分页顺序稳定
    traces.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });

    let total = traces.len();
    let items = traces.into_iter().skip(offset).take(limit).collect();
    Ok(PagedResult { items, total })
}

/// 列出待扫描的记忆目录（与写入侧布局对称：`agents/{agent_id}/memory`）
fn memory_trace_dirs(base_data_path: &Path, agent_id: Option<&str>) -> Result<Vec<PathBuf>> {
    if let Some(agent_id) = agent_id {
        return Ok(vec![paths::agent_memory_dir(base_data_path, agent_id)]);
    }
    let root = paths::agents_root_dir(base_data_path);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path().join("memory"));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// 列出目录下的 `.jsonl` 分片（目录不存在时返回空）
fn jsonl_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
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
fn date_in_range(path: &Path, created_after: Option<i64>, created_before: Option<i64>) -> bool {
    let Some(date) = path.file_stem().and_then(|s| s.to_str()) else {
        return true;
    };
    if let Some(after) = created_after.and_then(date_string)
        && date < after.as_str()
    {
        return false;
    }
    if let Some(before) = created_before.and_then(date_string)
        && date > before.as_str()
    {
        return false;
    }
    true
}

/// 秒级时间戳 → 落盘文件名使用的本地日期（YYYYMMDD）
fn date_string(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| {
        dt.with_timezone(&chrono::Local)
            .format("%Y%m%d")
            .to_string()
    })
}

/// 读取单文件并收集命中的轨迹（单行损坏只跳过，不让整次查询失败）
fn read_matching_traces(path: &Path, query: &MemoryTraceQuery, traces: &mut Vec<MemoryTrace>) {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => {
            log_warn!(
                "跳过无法打开的记忆轨迹文件: file={}, err={}",
                path.display(),
                err
            );
            return;
        }
    };
    for line in BufReader::new(file).lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                log_warn!("读取记忆轨迹行失败: file={}, err={}", path.display(), err);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let trace: MemoryTrace = match serde_json::from_str(&line) {
            Ok(trace) => trace,
            Err(err) => {
                log_warn!(
                    "跳过无法解析的记忆轨迹行: file={}, err={}",
                    path.display(),
                    err
                );
                continue;
            }
        };
        if matches_trace_query(&trace, query) {
            traces.push(trace);
        }
    }
}

/// 业务过滤：字段精确匹配 + 创建时间闭区间
fn matches_trace_query(trace: &MemoryTrace, query: &MemoryTraceQuery) -> bool {
    if let Some(task_id) = query.task_id.as_deref()
        && trace.task_id.as_deref() != Some(task_id)
    {
        return false;
    }
    if let Some(log_id) = query.log_id.as_deref()
        && trace.log_id != log_id
    {
        return false;
    }
    if let Some(trace_id) = query.trace_id.as_deref()
        && trace.id != trace_id
    {
        return false;
    }
    if let Some(created_after) = query.created_after
        && trace.created_at < created_after
    {
        return false;
    }
    if let Some(created_before) = query.created_before
        && trace.created_at > created_before
    {
        return false;
    }
    true
}

impl MemoryDaoSqliteImpl {
    pub(super) async fn append_trace(
        &self,
        _ctx: RequestContext,
        trace: &MemoryTrace,
    ) -> Result<MemoryTracePosition> {
        let agent_dir = self.agent_memory_dir(&trace.agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        let (date, line_number) = writer.append(trace)?;
        Ok(MemoryTracePosition {
            trace_id: trace.id.clone(),
            date_filename: format!("{date}.jsonl"),
            line_number: line_number as u64,
        })
    }

    pub(super) async fn batch_append_traces(
        &self,
        _ctx: RequestContext,
        traces: &[MemoryTrace],
    ) -> Result<Vec<MemoryTracePosition>> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }
        let agent_dir = self.agent_memory_dir(&traces[0].agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        // 一次数行 + 一次开文件：避免逐条 append 把成本放大成 O(n × 当日行数)
        let (date, first_line_number) = writer.append_batch(traces)?;
        let mut positions = Vec::with_capacity(traces.len());
        for (offset, trace) in traces.iter().enumerate() {
            let line_number = first_line_number + offset;
            positions.push(MemoryTracePosition {
                trace_id: trace.id.clone(),
                date_filename: format!("{date}.jsonl"),
                line_number: line_number as u64,
            });
        }
        Ok(positions)
    }

    pub(super) async fn query_traces(
        &self,
        _ctx: RequestContext,
        query: MemoryTraceQuery,
    ) -> Result<PagedResult<MemoryTrace>> {
        let base = config::get().base_data_path();
        query_traces_at(&base, &query)
    }

    pub(super) async fn get_trace(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_id: &str,
    ) -> Result<Option<MemoryTrace>> {
        let page = self
            .query_traces(
                ctx,
                MemoryTraceQuery {
                    agent_id: Some(agent_id.to_string()),
                    trace_id: Some(trace_id.to_string()),
                    pagination: PaginationParams {
                        limit: Some(1),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items.into_iter().next())
    }
}
