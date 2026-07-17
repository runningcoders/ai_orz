//! Log Query DAL 模块
//!
//! 职责：查询 JSONL 格式的应用日志文件，支持按关键词、log_id、级别、时间范围过滤。
//!
//! 日志文件由 tracing-appender 按日滚动生成，存放在 `{base_data_path}/logs/` 目录下，
//! 文件名格式为 `ai_orz.log.YYYY-MM-DD`，每行一个 JSON 对象。
//!
//! 每条日志的 JSON 结构（tracing-subscriber json 格式）：
//! ```json
//! {
//!   "timestamp": "2026-06-26T12:12:48.829076Z",
//!   "level": "INFO",
//!   "fields": {
//!     "message": "...",
//!     "log_id": "...",
//!     "user_id": "...",
//!     "operation": "..."
//!   },
//!   "target": "ai_orz::pkg",
//!   "filename": "src/pkg/mod.rs",
//!   "line_number": 35
//! }
//! ```

use crate::config;
use crate::pkg::RequestContext;
use common::error::Result;
use std::sync::{Arc, OnceLock};

use chrono::{DateTime, NaiveDate, Utc};

// ==================== 数据结构 ====================

/// 单条日志条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    /// ISO8601 格式时间戳
    pub timestamp: String,
    /// 日志级别（INFO / WARN / ERROR / DEBUG / TRACE）
    pub level: String,
    /// 日志消息
    pub message: String,
    /// 请求追踪 ID（来自 fields.log_id）
    pub log_id: Option<String>,
    /// 用户 ID（来自 fields.user_id）
    pub user_id: Option<String>,
    /// 操作名称（来自 fields.operation）
    pub operation: Option<String>,
    /// 原始 JSON 对象
    pub raw: serde_json::Value,
}

/// 日志查询参数
pub struct LogQuery {
    /// 关键词（message 字段包含，不区分大小写）
    pub keyword: Option<String>,
    /// 调用链 ID 精确匹配
    pub log_id: Option<String>,
    /// 日志级别过滤（INFO / WARN / ERROR / DEBUG）
    pub level: Option<String>,
    /// 起始时间（unix timestamp ms，含）
    pub start_time: Option<i64>,
    /// 结束时间（unix timestamp ms，含）
    pub end_time: Option<i64>,
    /// 页码（从 1 开始）
    pub page: usize,
    /// 每页条数
    pub page_size: usize,
}

/// 分页结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogPageResult {
    /// 匹配总数（最多 MAX_SCAN_ENTRIES）
    pub total: usize,
    /// 当前页日志条目
    pub entries: Vec<LogEntry>,
    /// 当前页码
    pub page: usize,
    /// 每页条数
    pub page_size: usize,
}

// ==================== 单例管理 ====================

static LOG_QUERY_DAL: OnceLock<Arc<dyn LogQueryDal + Send + Sync>> = OnceLock::new();

/// 获取 LogQuery DAL 单例
pub fn dal() -> Arc<dyn LogQueryDal + Send + Sync> {
    LOG_QUERY_DAL.get().cloned().unwrap()
}

/// 初始化 LogQuery DAL
pub fn init() {
    let _ = LOG_QUERY_DAL.set(Arc::new(LogQueryDalFsImpl));
}

// ==================== DAL 接口 ====================

/// LogQuery DAL 接口
#[async_trait::async_trait]
pub trait LogQueryDal: Send + Sync {
    /// 查询日志，返回分页结果（按时间倒序，最新的在前）
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<LogPageResult>;
}

// ==================== DAL 实现 ====================

/// 基于文件系统的 LogQuery DAL 实现
struct LogQueryDalFsImpl;

/// 日志文件名前缀（tracing-appender daily rolling 生成 `ai_orz.log.YYYY-MM-DD`）
const LOG_FILE_PREFIX: &str = "ai_orz.log.";
/// 单次查询最多收集的匹配记录数（防止内存溢出）
const MAX_SCAN_ENTRIES: usize = 10000;
/// 最多扫描最近 N 天的日志文件
const MAX_SCAN_DAYS: i64 = 30;

#[async_trait::async_trait]
impl LogQueryDal for LogQueryDalFsImpl {
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<LogPageResult> {
        let _ = ctx;

        // 规范化分页参数
        let page = if query.page == 0 { 1 } else { query.page };
        let page_size = if query.page_size == 0 {
            20
        } else {
            query.page_size
        };

        let logs_dir = config::get().log_dir();

        if !logs_dir.exists() {
            return Ok(LogPageResult {
                total: 0,
                entries: Vec::new(),
                page,
                page_size,
            });
        }

        // 收集日志文件并按日期倒序排列（最新文件优先扫描）
        let mut log_files = collect_log_files(&logs_dir);
        log_files.sort_by(|a, b| b.cmp(a));

        // 预处理过滤条件
        let keyword_lower = query.keyword.as_ref().map(|s| s.to_lowercase());
        let level_filter = query.level.as_ref().map(|s| s.to_uppercase());

        let mut entries: Vec<LogEntry> = Vec::new();

        for file_path in &log_files {
            if entries.len() >= MAX_SCAN_ENTRIES {
                break;
            }

            let file = match std::fs::File::open(file_path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for line in std::io::BufRead::lines(std::io::BufReader::new(file)) {
                if entries.len() >= MAX_SCAN_ENTRIES {
                    break;
                }

                let line = match line {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let raw: serde_json::Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if let Some(entry) = parse_and_filter(
                    &raw,
                    keyword_lower.as_deref(),
                    query.log_id.as_deref(),
                    level_filter.as_deref(),
                    query.start_time,
                    query.end_time,
                ) {
                    entries.push(entry);
                }
            }
        }

        // 按时间倒序排列（最新的在前）
        // ISO8601 格式字符串可直接按字典序比较得到时间顺序
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let total = entries.len();
        let skip = (page - 1) * page_size;
        let page_entries: Vec<LogEntry> =
            entries.into_iter().skip(skip).take(page_size).collect();

        Ok(LogPageResult {
            total,
            entries: page_entries,
            page,
            page_size,
        })
    }
}

// ==================== 辅助函数 ====================

/// 收集日志目录下所有匹配 `ai_orz.log.YYYY-MM-DD` 的文件路径，
/// 仅保留最近 MAX_SCAN_DAYS 天内的文件。
fn collect_log_files(logs_dir: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();

    let entries = match std::fs::read_dir(logs_dir) {
        Ok(e) => e,
        Err(_) => return files,
    };

    let today = Utc::now().date_naive();
    let min_date = today - chrono::Duration::days(MAX_SCAN_DAYS);

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        // 解析文件名中的日期（ai_orz.log.YYYY-MM-DD）
        let date_str = match filename.strip_prefix(LOG_FILE_PREFIX) {
            Some(d) => d,
            None => continue,
        };

        let file_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        // 仅保留最近 MAX_SCAN_DAYS 天的文件
        if file_date < min_date {
            continue;
        }

        files.push(path.to_string_lossy().to_string());
    }

    files
}

/// 从原始 JSON 解析日志条目，并应用过滤条件。
///
/// 返回 `Some(entry)` 表示通过所有过滤条件，`None` 表示不匹配或关键字段缺失。
fn parse_and_filter(
    raw: &serde_json::Value,
    keyword_lower: Option<&str>,
    log_id_filter: Option<&str>,
    level_filter: Option<&str>,
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Option<LogEntry> {
    // timestamp - 顶层字段
    let timestamp = raw
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // level - 顶层字段
    let level = raw.get("level").and_then(|v| v.as_str()).unwrap_or("");

    // fields 子对象（tracing-subscriber json 格式将 span/event 字段放在 fields 中）
    let fields = raw.get("fields");

    // message - 在 fields.message
    let message = fields
        .and_then(|f| f.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // log_id - 在 fields.log_id（#[log_field] 注入的 span 字段）
    let log_id = fields
        .and_then(|f| f.get("log_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // user_id - 在 fields.user_id
    let user_id = fields
        .and_then(|f| f.get("user_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // operation - 在 fields.operation
    let operation = fields
        .and_then(|f| f.get("operation"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // ---- 过滤条件 ----

    // level 过滤（不区分大小写）
    if let Some(lf) = level_filter {
        if level.to_uppercase() != lf {
            return None;
        }
    }

    // log_id 过滤（精确匹配）
    if let Some(filter_id) = log_id_filter {
        if log_id.as_deref() != Some(filter_id) {
            return None;
        }
    }

    // keyword 过滤（message 不区分大小写包含）
    if let Some(kw) = keyword_lower {
        if !message.to_lowercase().contains(kw) {
            return None;
        }
    }

    // 时间范围过滤（需要解析 timestamp 为 unix 毫秒）
    if start_time.is_some() || end_time.is_some() {
        let ts_ms = parse_timestamp_to_millis(timestamp)?;
        if let Some(start) = start_time {
            if ts_ms < start {
                return None;
            }
        }
        if let Some(end) = end_time {
            if ts_ms > end {
                return None;
            }
        }
    }

    Some(LogEntry {
        timestamp: timestamp.to_string(),
        level: level.to_string(),
        message: message.to_string(),
        log_id,
        user_id,
        operation,
        raw: raw.clone(),
    })
}

/// 解析 ISO8601/RFC3339 时间戳为 unix 毫秒
fn parse_timestamp_to_millis(ts: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp_millis())
}
