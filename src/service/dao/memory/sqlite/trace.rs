//! 记忆轨迹 DAO：写入每日 JSONL + 旁车索引点查

#[path = "trace_sidecar.rs"]
mod trace_sidecar;

use super::MemoryDaoSqliteImpl;
use crate::config;
use crate::models::memory::{MemoryTrace, MemoryTracePosition};
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::service::dao::memory::MemoryTraceQuery;
use common::api::{PagedResult, PaginationParams};
use common::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use trace_sidecar::TraceSidecar;

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

    // 【索引路径】每个 agent 目录一个旁车索引：索引层过滤 + 日期粗筛后只 seek 读命中行。
    // 索引缺失/落后/损坏时由底层增量重建（兼容直接写原文的存量数据与测试夹具）。
    let mut traces = Vec::new();
    for dir in memory_trace_dirs(base_data_path, query.agent_id.as_deref())? {
        let sidecar = TraceSidecar::new(dir.clone());
        for hit in sidecar.query(query)? {
            match sidecar.read_trace(&hit.date, hit.offset, hit.len) {
                Ok(trace) => traces.push(trace),
                // 索引指向的行损坏/被清理：跳过并告警，不让一次坏行拖垮整次查询。
                // 与旧「单行损坏跳过」口径一致。
                Err(err) => log_warn!(
                    "跳过无法读取的记忆轨迹行: dir={}, date={}, offset={}, err={}",
                    dir.display(),
                    hit.date,
                    hit.offset,
                    err
                ),
            }
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

/// 按 trace_id 集合解析各自的落盘物理位置（经旁车索引点查）
///
/// 与 [`query_traces_at`] 的分工（**别用后者来定位**）：
/// - `query_traces_at` 是**列表/详情查询**，受分页上限约束（`MAX_MEMORY_TRACE_QUERY_LIMIT`），
///   且 `MemoryTrace.position` 带 `#[serde(skip)]`、查询侧不回填 ⇒ **拿不到行号**；
/// - 本函数是**精确定位**：给定一组 trace_id 直接回各自的 `(date_path, line_number)`，
///   不受分页上限约束。它服务的正是「知识节点 → 短期记忆 → trace → 原始记录」这条引用链——
///   `KnowledgeReferencePo` 有了 date_path + line_number 才能 `read_memory_reference` 读回原文。
///
/// 未命中的 trace_id **缺席于结果**而不报错：trace 可能因日志轮转/清理而读不到，
/// 但「引用过它」这个事实仍然成立，引用关系照写，只是没有物理坐标。
pub(crate) fn resolve_trace_positions_at(
    base_data_path: &Path,
    agent_id: &str,
    trace_ids: &[String],
) -> Result<HashMap<String, MemoryTracePosition>> {
    let mut found = HashMap::with_capacity(trace_ids.len());
    if trace_ids.is_empty() {
        return Ok(found);
    }

    // 【索引路径】line_number 随索引记录落盘，按 trace_id 命中即得物理坐标，不扫原文。
    let wanted_ids: Vec<&str> = trace_ids.iter().map(String::as_str).collect();
    for dir in memory_trace_dirs(base_data_path, Some(agent_id))? {
        let sidecar = TraceSidecar::new(dir);
        for hit in sidecar.resolve_positions(&wanted_ids)? {
            if found.contains_key(&hit.trace_id) {
                continue;
            }
            found.insert(
                hit.trace_id.clone(),
                MemoryTracePosition {
                    trace_id: hit.trace_id,
                    date_filename: hit.date_filename,
                    line_number: hit.line_number,
                },
            );
        }
    }
    Ok(found)
}

impl MemoryDaoSqliteImpl {
    pub(super) async fn append_trace(
        &self,
        _ctx: RequestContext,
        trace: &MemoryTrace,
    ) -> Result<MemoryTracePosition> {
        let agent_dir = self.agent_memory_dir(&trace.agent_id);
        let writer = crate::pkg::jsonl::writer::DailyJsonlWriter::new(&agent_dir);
        let sidecar = TraceSidecar::new(agent_dir);
        let trace_ref = trace;
        // 原文 append + 索引 append 在同一次持锁内完成（行号/偏移与索引行严格对应）；
        // 索引写失败只告警、不传播——原文已落盘，读路径会补齐。
        let (date, line_number) = writer.append_with_index_position(
            std::slice::from_ref(trace_ref),
            |date, line, offset, len| {
                if let Err(err) = sidecar.append_record(date, trace_ref, line, offset, len) {
                    log_warn!(
                        "记忆轨迹索引追加失败，将在下次查询补齐: date={}, trace_id={}, err={}",
                        date,
                        trace.id,
                        err
                    );
                }
                Ok(())
            },
        )?;
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
        let writer = crate::pkg::jsonl::writer::DailyJsonlWriter::new(&agent_dir);
        let sidecar = TraceSidecar::new(agent_dir);
        // 一次数行 + 一次开文件 + 逐条索引，全在同一把锁内（避免逐条 append 放大成
        // O(n × 当日行数)，也保证行号/偏移与索引行严格对应）
        let mut idx = 0usize;
        let (date, first_line_number) =
            writer.append_with_index_position(traces, |date, line, offset, len| {
                if let Err(err) = sidecar.append_record(date, &traces[idx], line, offset, len) {
                    log_warn!(
                        "记忆轨迹索引追加失败，将在下次查询补齐: date={}, line={}, trace_id={}, err={}",
                        date,
                        line,
                        traces[idx].id,
                        err
                    );
                }
                idx += 1;
                Ok(())
            })?;
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

    pub(super) async fn resolve_trace_positions(
        &self,
        _ctx: RequestContext,
        agent_id: &str,
        trace_ids: &[String],
    ) -> Result<HashMap<String, MemoryTracePosition>> {
        let base = config::get().base_data_path();
        resolve_trace_positions_at(&base, agent_id, trace_ids)
    }
}
