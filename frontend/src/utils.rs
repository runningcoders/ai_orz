//! 通用工具函数 - 时间格式化、文件大小、消息辅助、状态映射
//! 提取自各页面中重复定义的工具函数。

use std::time::{SystemTime, UNIX_EPOCH};
use web_sys::window;

pub fn local_storage() -> Option<web_sys::Storage> {
    window()?.local_storage().ok()?
}

// ============================================================================
// 时间格式化
// ============================================================================

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 毫秒时间戳 → "HH:MM"（本地时区，解析失败返回 "--:--"）
pub fn format_time_hm(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts_ms / 1000, 0) {
        chrono::LocalResult::Single(dt) => format!("{}", dt.format("%H:%M")),
        _ => "--:--".to_string(),
    }
}

/// 毫秒时间戳 → "YYYY-MM-DD HH:MM"（本地时区，解析失败回退为原始值）
pub fn format_datetime(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts_ms / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts_ms.to_string())
}

/// 毫秒时间戳 → "YYYY-MM-DD HH:MM:SS"（本地时区，解析失败回退为原始值）
pub fn format_datetime_full(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(ts_ms / 1000, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts_ms.to_string())
}

/// 可选毫秒时间戳 → "YYYY-MM-DD HH:MM:SS"，None 返回 "—"
pub fn format_timestamp_opt(ts_ms: Option<i64>) -> String {
    ts_ms
        .map(format_datetime_full)
        .unwrap_or_else(|| "—".to_string())
}

/// RFC3339/ISO8601 字符串 → "YYYY-MM-DD HH:MM:SS"（本地时区，解析失败原样返回）
pub fn format_rfc3339(ts: &str) -> String {
    if ts.is_empty() {
        return "-".to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if ts.len() >= 19 {
        return ts[..19].replace('T', " ");
    }
    ts.to_string()
}

// ============================================================================
// ID 生成
// ============================================================================

/// 生成乐观消息的临时 ID（tmp_<ms>_<random>，避免同毫秒碰撞）
pub fn tmp_msg_id() -> String {
    let random = (js_sys::Math::random() * 1_000_000_000.0) as u32;
    format!("tmp_{}_{:09}", now_ms(), random)
}

// ============================================================================
// 文件大小
// ============================================================================

/// 字节数 → 人类可读（B/KB/MB/GB，1 位小数）
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ============================================================================
// 消息辅助
// ============================================================================

/// 消息类型常量
pub const MSG_TEXT: i32 = 0;
pub const MSG_IMAGE: i32 = 1;
pub const MSG_FILE: i32 = 2;
pub const MSG_AUDIO: i32 = 3;
pub const MSG_VIDEO: i32 = 4;
pub const MSG_TOOL_CALL_REQUEST: i32 = 5;
pub const MSG_TOOL_CALL_RESULT: i32 = 6;
pub const MSG_TASK_ASSIGNMENT: i32 = 9;

/// 判断是否为附件消息（图片/文件/音频/视频）
pub fn is_attachment_message(msg_type: i32) -> bool {
    matches!(msg_type, MSG_IMAGE | MSG_FILE | MSG_AUDIO | MSG_VIDEO)
}

/// 角色 → 头像字符（0=User, 1=Agent, 2=System）
pub fn role_avatar(role: i32) -> &'static str {
    match role {
        0 => "U",
        1 => "A",
        2 => "S",
        _ => "?",
    }
}

// ============================================================================
// 任务状态映射
// ============================================================================

/// 任务状态文本（0=已取消, 1=待审核, 2=待处理, 3=进行中, 4=已完成, 5=已归档）
pub fn task_status_text(status: i32) -> &'static str {
    match status {
        0 => "已取消",
        1 => "待审核",
        2 => "待处理",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

/// 任务状态徽章 class
pub fn task_status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-warning",
        2 => "badge badge-info",
        3 => "badge badge-primary",
        4 => "badge badge-success",
        5 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}

/// 进度条 class（0-25=warning, 26-50=primary, 51-75=accent, 76-100=success）
pub fn progress_bar_class(progress: i32) -> &'static str {
    match progress {
        0..=25 => "overview-progress-fill warning",
        26..=50 => "overview-progress-fill primary",
        51..=75 => "overview-progress-fill accent",
        76..=100 => "overview-progress-fill success",
        _ => "overview-progress-fill",
    }
}

// ============================================================================
// 项目状态映射
// ============================================================================

/// 项目状态文本（0=已删除, 1=活跃, 2=待审核, 3=进行中, 4=已完成, 5=已归档）
pub fn project_status_text(status: i32) -> &'static str {
    match status {
        0 => "已删除",
        1 => "活跃",
        2 => "待审核",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

/// 项目状态徽章 class（0=error, 1=info, 2=warning, 3=primary, 4=success, 5=neutral）
pub fn project_status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-info",
        2 => "badge badge-warning",
        3 => "badge badge-primary",
        4 => "badge badge-success",
        5 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}
