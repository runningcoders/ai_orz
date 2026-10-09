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
///
/// 协议化改造：定义收敛到 `common::api::LogEntry`（前后端共享），
/// 此处保留 re-export 以兼容 domain 层既有导入路径。
pub use common::api::{LogEntry, QueryLogsResponse};

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

/// 创建 LogQuery DAL（返回 trait 对象，用于测试或组合构造）
pub fn new() -> Arc<dyn LogQueryDal + Send + Sync> {
    Arc::new(LogQueryDalFsImpl)
}

// ==================== DAL 接口 ====================

/// LogQuery DAL 接口
#[async_trait::async_trait]
pub trait LogQueryDal: Send + Sync {
    /// 查询日志，返回分页结果（按时间倒序，最新的在前）
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<QueryLogsResponse>;
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
/// 反向扫描的块缓冲大小（案 B：seek 尾部 + 固定块缓冲反向解析）
const SCAN_CHUNK_SIZE: usize = 64 * 1024;

#[async_trait::async_trait]
impl LogQueryDal for LogQueryDalFsImpl {
    async fn query_logs(&self, ctx: RequestContext, query: LogQuery) -> Result<QueryLogsResponse> {
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
            return Ok(QueryLogsResponse {
                total: 0,
                entries: Vec::new(),
                page,
                page_size,
            });
        }

        // 目录内查询（收集 → 倒序扫描 → 终排 → 分页）抽为独立函数：
        // 日志目录来自全局配置单例，单测无法注入临时目录；抽参后主链路可测。
        query_logs_in_dir(&logs_dir, &query, page, page_size)
    }
}

/// 目录内执行日志查询：收集日志文件（新文件优先）→ 文件内倒序扫描
/// （案 B：seek 尾部 + 固定块缓冲反向解析；收集满 `MAX_SCAN_ENTRIES` 即停，
/// 停机语义 = 收集「最新」的 10000 条）→ 按时间倒序终排 → 内存分页。
///
/// 从 `query_logs` 抽出的原因：日志目录来自全局配置单例，测试无法注入；
/// 显式 `logs_dir` 参数使主链路单测可直接以临时目录调用。
fn query_logs_in_dir(
    logs_dir: &std::path::Path,
    query: &LogQuery,
    page: usize,
    page_size: usize,
) -> Result<QueryLogsResponse> {
    // 收集日志文件并按日期倒序排列（最新文件优先扫描）
    let mut log_files = collect_log_files(logs_dir);
    log_files.sort_by(|a, b| b.cmp(a));

    // 预处理过滤条件
    let keyword_lower = query.keyword.as_ref().map(|s| s.to_lowercase());
    let level_filter = query.level.as_ref().map(|s| s.to_uppercase());

    let mut entries: Vec<LogEntry> = Vec::new();

    for file_path in &log_files {
        // 文件级停机：已收集满上限，不再打开后续（更旧的）文件
        if entries.len() >= MAX_SCAN_ENTRIES {
            break;
        }

        let Ok(file) = std::fs::File::open(file_path) else {
            continue;
        };
        let file_len = match file.metadata() {
            Ok(m) => m.len(),
            Err(_) => continue,
        };

        // 块读取闭包：从 offset 处读满 buf，返回实际填充长度（短读=源比预期短）
        let read_chunk = |offset: u64, buf: &mut [u8]| -> std::io::Result<usize> {
            use std::io::{Read, Seek, SeekFrom};
            let mut f = &file;
            f.seek(SeekFrom::Start(offset))?;
            let mut filled = 0;
            while filled < buf.len() {
                match f.read(&mut buf[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(filled)
        };

        // 行处理闭包：解析 + 过滤 + 收集；收集满上限返回 false 提前停机（行级停机）
        let on_line = |line_bytes: &[u8]| -> bool {
            if entries.len() >= MAX_SCAN_ENTRIES {
                return false;
            }
            // 非 UTF-8 行跳过（与正序 BufRead::lines 的 Err-continue 语义等价）
            let Ok(line) = std::str::from_utf8(line_bytes) else {
                return true;
            };
            let line = line.trim();
            if line.is_empty() {
                return true;
            }
            let Ok(raw) = serde_json::from_str::<serde_json::Value>(line) else {
                return true;
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
            entries.len() < MAX_SCAN_ENTRIES
        };

        // 块级 IO 异常时跳过该文件继续（与正序版逐行 Err-continue 的容错口径一致）
        if scan_lines_reverse(file_len, read_chunk, on_line).is_err() {
            continue;
        }
    }

    // 按时间倒序排列（最新的在前）
    // ISO8601 格式字符串可直接按字典序比较得到时间顺序
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total = entries.len();
    let skip = (page - 1) * page_size;
    let page_entries: Vec<LogEntry> = entries.into_iter().skip(skip).take(page_size).collect();

    Ok(QueryLogsResponse {
        total,
        entries: page_entries,
        page,
        page_size,
    })
}

/// 反向逐行扫描一个字节源（案 B：seek 尾部 + 固定块缓冲反向解析）。
///
/// 从源末尾向前以 [`SCAN_CHUNK_SIZE`] 块读取，按 `\n` 切行并以**倒序**
/// （最新行优先）逐行交给 `on_line`；`on_line` 返回 `false` 时立即终止。
///
/// 与正序 `BufRead::lines` 的语义对齐（差异经调用方 `trim` 后等价）：
/// - 文件尾无换行的最后一行正常产出；
/// - 行跨块边界时正确拼接（不截断、不重复）；
/// - 连续换行 / 块尾换行会产出空行片段（由调用方跳过，行为等价）；
/// - 行尾 `\r` 保留（调用方 `trim` 去除）。
///
/// `read_chunk(offset, buf)` 读取 `[offset, offset + buf.len())` 的内容填入
/// `buf`，返回实际填充长度（短读表示源比预期短，如并发截断）。该闭包抽象
/// 使单测可用内存 `&[u8]` 字节源注入，无需真实文件。
fn scan_lines_reverse(
    source_len: u64,
    mut read_chunk: impl FnMut(u64, &mut [u8]) -> std::io::Result<usize>,
    mut on_line: impl FnMut(&[u8]) -> bool,
) -> std::io::Result<()> {
    // carry：跨块边界的行头片段（当前读取位置之前、尚未遇到其换行符的行开头部分）
    let mut carry: Vec<u8> = Vec::new();
    let mut pos = source_len;

    while pos > 0 {
        let start = pos.saturating_sub(SCAN_CHUNK_SIZE as u64);
        let want = (pos - start) as usize;
        let mut buf = vec![0u8; want];
        let filled = read_chunk(start, &mut buf)?;
        if filled == 0 {
            break; // 源比预期短（并发截断等），按已读内容收尾
        }
        buf.truncate(filled);

        let mut end = buf.len();
        while end > 0 {
            match buf[..end].iter().rposition(|&b| b == b'\n') {
                Some(nl) => {
                    // 完整行 = 本块内片段（较早内容）+ carry（已扫过的更晚块中的片段）
                    let mut line = buf[nl + 1..end].to_vec();
                    line.extend_from_slice(&carry);
                    carry.clear();
                    if !on_line(&line) {
                        return Ok(());
                    }
                    end = nl;
                }
                None => {
                    // 本块剩余部分无换行：整体并入行头片段，待读更早内容后拼接
                    let mut head = buf[..end].to_vec();
                    head.extend_from_slice(&carry);
                    carry = head;
                    end = 0;
                }
            }
        }
        pos = start;
    }

    // 源头残余：carry 即源中的第一行（其前再无内容）
    if !carry.is_empty() {
        on_line(&carry);
    }
    Ok(())
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
    let timestamp = raw.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");

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
    if let Some(lf) = level_filter
        && level.to_uppercase() != lf
    {
        return None;
    }

    // log_id 过滤（精确匹配）
    if let Some(filter_id) = log_id_filter
        && log_id.as_deref() != Some(filter_id)
    {
        return None;
    }

    // keyword 过滤（message 不区分大小写包含）
    if let Some(kw) = keyword_lower
        && !message.to_lowercase().contains(kw)
    {
        return None;
    }

    // 时间范围过滤（需要解析 timestamp 为 unix 毫秒）
    if start_time.is_some() || end_time.is_some() {
        let ts_ms = parse_timestamp_to_millis(timestamp)?;
        if let Some(start) = start_time
            && ts_ms < start
        {
            return None;
        }
        if let Some(end) = end_time
            && ts_ms > end
        {
            return None;
        }
    }

    Some(LogEntry {
        timestamp: timestamp.to_string(),
        level: level.to_string(),
        message: message.to_string(),
        log_id,
        user_id,
        operation,
        raw: Some(raw.clone()),
    })
}

/// 解析 ISO8601/RFC3339 时间戳为 unix 毫秒
fn parse_timestamp_to_millis(ts: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.timestamp_millis())
}
#[cfg(test)]
#[path = "log_query_tests.rs"]
mod tests;
