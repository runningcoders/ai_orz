// ==================== 日志配置 ====================
//! 日志模块：同时输出到控制台和按日期自动分割的日志文件
//!
//! - 控制台输出：方便开发调试
//! - 文件输出：按日期自动滚动，持久化日志
//! - 支持 JSON 格式输出，便于日志分析
//! - 支持日志自动清理（保留 N 天）
//! - 日志路径从应用配置读取，支持自定义数据目录

use std::fs;
use std::time::Duration;

use common::config::AppConfig;
use once_cell::sync::OnceCell;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// 全局持有 tracing non-blocking writer 的 guard
/// 保证程序退出前所有日志都被 flush 到磁盘
static WORKER_GUARD: OnceCell<WorkerGuard> = OnceCell::new();

/// 初始化日志系统
///
/// - 同时输出到控制台和配置的日志目录下按日期自动分割的日志文件
/// - 自动按日期滚动，不会产生过大日志文件
/// - 支持 JSON 格式输出
/// - 支持日志自动清理（保留 N 天）
pub fn init(config: &AppConfig) {
    // 日志格式配置
    let is_json_format = config.logging.format.to_lowercase() == "json";

    // 过滤层：从环境变量读取，默认 info 级别
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 如果启用文件日志，输出到配置的目录
    if config.logging.enable_file_log {
        let logs_dir = config.log_dir();
        // 创建日志目录（如果不存在，load_config 已经创建过，但保险起见再检查一次）
        if !logs_dir.exists() {
            std::fs::create_dir_all(&logs_dir)
                .unwrap_or_else(|_| panic!("Failed to create logs directory at {:?}", logs_dir));
        }

        // 自动清理旧日志
        if config.logging.retention_days > 0 {
            let retention_period =
                Duration::from_secs(config.logging.retention_days as u64 * 86400);
            let _ = cleanup_old_logs(&logs_dir, retention_period);
        }

        // 文件输出层：按日期自动滚动，每天新建一个日志文件
        let file_appender = rolling::daily(&logs_dir, "ai_orz.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // 根据格式选择输出方式 - 独立构建 layer，避免类型不匹配
        if is_json_format {
            // JSON 格式：控制台 + 文件
            let console_layer = fmt::layer()
                .json()
                .with_target(true)
                .with_file(true)
                .with_line_number(true);

            let file_layer = fmt::layer()
                .json()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(non_blocking);

            tracing_subscriber::registry()
                .with(filter_layer)
                .with(console_layer)
                .with(file_layer)
                .init();
        } else {
            // 文本格式：控制台 + 文件
            let console_layer = fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true);

            let file_layer = fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true)
                .with_writer(non_blocking);

            tracing_subscriber::registry()
                .with(filter_layer)
                .with(console_layer)
                .with(file_layer)
                .init();
        }

        // 全局持有 guard，保证程序运行期间不会被 drop
        let _ = WORKER_GUARD.set(guard);
    } else {
        // 只输出到控制台
        if is_json_format {
            let console_layer = fmt::layer()
                .json()
                .with_target(true)
                .with_file(true)
                .with_line_number(true);

            tracing_subscriber::registry()
                .with(filter_layer)
                .with(console_layer)
                .init();
        } else {
            let console_layer = fmt::layer()
                .with_target(true)
                .with_file(true)
                .with_line_number(true);

            tracing_subscriber::registry()
                .with(filter_layer)
                .with(console_layer)
                .init();
        }
    }
}

/// 清理过期日志文件
///
/// 删除目录中超过保留期限的日志文件
fn cleanup_old_logs(logs_dir: &std::path::Path, retention: Duration) -> std::io::Result<usize> {
    let now = std::time::SystemTime::now();
    let mut deleted_count = 0;

    for entry in fs::read_dir(logs_dir)? {
        let entry = entry?;
        let path = entry.path();

        // 只处理日志文件（文件名以 ai_orz.log. 开头）
        if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
            if !filename.starts_with("ai_orz.log.") {
                continue;
            }
        } else {
            continue;
        }

        // 检查文件修改时间
        if let Ok(metadata) = fs::metadata(&path)
            && let Ok(modified) = metadata.modified()
            && let Ok(elapsed) = now.duration_since(modified)
            && elapsed > retention
        {
            let _ = fs::remove_file(&path);
            deleted_count += 1;
        }
    }

    Ok(deleted_count)
}

/// 日志字段注入 trait
///
/// 通过 `#[derive(LogFields)]` 自动实现，标注 `#[log_field]` 的字段
/// 会被自动注入到 tracing span 中。
///
/// 字段列表是单一数据源：只在 struct 定义处维护，新增字段加 `#[log_field]` 即可。
pub trait LogFields {
    /// 创建包含所有标注字段的 tracing span
    ///
    /// 日志宏内部调用此方法，传入日志级别和操作名称
    fn create_log_span(&self, operation: &str, level: tracing::Level) -> tracing::Span;
}

// ==================== 敏感信息脱敏 ====================

/// 敏感字段名（小写子串匹配，命中即把值替换为 `***`）
///
/// 单一数据源：JSON 结构化脱敏与文本 KV 模式脱敏共用此列表，
/// 新增敏感字段只需在此追加。
pub const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "api_key",
    "apikey",
    "token",
    "secret",
    "authorization",
    "credential",
];

/// 判断字段名是否命中敏感字段（小写子串匹配，如 `chat_model.api_key` → `api_key` 命中）
pub fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    SENSITIVE_KEYS.iter().any(|k| key_lower.contains(k))
}

/// 递归脱敏 JSON：命中敏感字段的值替换为 `***`（就地修改）
///
/// 适用于能解析为 JSON 结构的日志内容（请求/响应体、DTO 调试输出等）。
/// 命中敏感键（password/token/...）的字段值整体替换为 `***`；非敏感键下的
/// 字符串值（如 shell 命令参数、错误文本）递归走 [`mask_sensitive_text`] 做
/// 文本级脱敏（支持 `key=value` / `key: value` / `"key":"value"` / `--key value` / Bearer）。
pub fn mask_sensitive_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *val = serde_json::Value::String("***".to_string());
                } else {
                    mask_sensitive_json(val);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                mask_sensitive_json(item);
            }
        }
        serde_json::Value::String(s) => {
            // 递归脱敏字符串值内部自由文本（如 shell 命令参数 `--token xxx`、
            // 错误信息中的 `key=value`）。敏感键的值已在 Object 分支整体替换，此处
            // 只处理非敏感键下承载的裸字符串值。
            let masked = mask_sensitive_text(s);
            *s = masked;
        }
        _ => {}
    }
}

/// 文本级脱敏：在任意文本中扫描「敏感键 + 分隔符 + 值」模式，把值替换为 `***`
///
/// 适用于无法结构化解析的文本日志（错误信息、非 JSON 请求体、第三方返回文本等）。
/// 支持的值形态：
/// - `key=value` / `key: value`（裸值，止于空白、逗号、分号、`}`、`]`、引号）
/// - `"key":"value"`（JSON 字符串形态，含转义）
/// - `Authorization: Bearer xxx`（Bearer 后的第二个 token 一并脱敏）
pub fn mask_sensitive_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < bytes.len() {
        let mut matched = false;
        for key in SENSITIVE_KEYS {
            let key_bytes = key.as_bytes();
            let key_end = i + key_bytes.len();
            if key_end > bytes.len() || !bytes[i..key_end].eq_ignore_ascii_case(key_bytes) {
                continue;
            }
            // 键后：可选空格 + 分隔符（= / : / 收尾引号 JSON 形态 / 空格 flag）
            let mut j = key_end;
            let had_space = j < bytes.len() && bytes[j] == b' ';
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j >= bytes.len() {
                continue;
            }
            let sep = bytes[j];
            let value_start;
            if sep == b'"' {
                // JSON 字符串形态："key":"value"（key 收尾引号后为 :）
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b':') {
                    j += 1;
                }
                value_start = j; // 值 opening quote 位置（text[i..value_start] 含 key 引号+冒号）
                // 扫描值内容（跳过 opening quote，处理转义）
                let mut vj = j + 1;
                while vj < bytes.len() && bytes[vj] != b'"' {
                    if bytes[vj] == b'\\' {
                        vj += 1; // 跳过转义符
                    }
                    vj += 1;
                }
                let value_end = (vj + 1).min(bytes.len()); // 含收尾引号
                if value_end > value_start {
                    out.push_str(&text[i..value_start]);
                    out.push_str("***");
                    i = value_end;
                    matched = true;
                }
                break;
            } else if sep == b'=' || sep == b':' {
                // KV 形态：key=value / key: value
                j += 1;
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                value_start = j;
            } else if had_space && i > 0 && bytes[i - 1] == b'-' {
                // CLI flag 形态：`--key value`（key 前为 `-`，空格分隔、无 =/:）
                // 注意：上方已跳过 key 后空格，j 已指向值首字符，勿再 j += 1
                value_start = j;
                // flag value 以 `-` 开头表示下一个 flag，不脱敏（如 `--token --verbose`）
                if bytes.get(value_start) == Some(&b'-') {
                    continue;
                }
            } else {
                continue;
            }

            // 裸值 / 引号值 / flag 值：止于分隔字符
            let mut value_end = value_start;
            if bytes.get(value_start) == Some(&b'"') {
                // 引号包裹值（如 key="value"）
                j = value_start + 1;
                while j < bytes.len() && bytes[j] != b'"' {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                value_end = (j + 1).min(bytes.len());
            } else {
                while value_end < bytes.len() {
                    let c = bytes[value_end];
                    if c.is_ascii_whitespace()
                        || matches!(c, b',' | b';' | b'}' | b']' | b'"' | b'\'')
                    {
                        break;
                    }
                    value_end += 1;
                }
                // `Bearer xxx` —— 再吞掉一个空白分隔的 token
                if bytes[value_start..value_end].eq_ignore_ascii_case(b"bearer") {
                    let mut k = value_end;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    while k < bytes.len() {
                        let c = bytes[k];
                        if c.is_ascii_whitespace()
                            || matches!(c, b',' | b';' | b'}' | b']' | b'"' | b'\'')
                        {
                            break;
                        }
                        k += 1;
                    }
                    if k > value_end {
                        value_end = k;
                    }
                }
            }

            if value_end > value_start {
                out.push_str(&text[i..value_start]);
                out.push_str("***");
                i = value_end;
                matched = true;
            }
            break;
        }

        if !matched {
            // 逐字符推进（多字节 UTF-8 一次推一个 char，避免切断字符边界）
            let mut next = i + 1;
            while next < bytes.len() && !text.is_char_boundary(next) {
                next += 1;
            }
            out.push_str(&text[i..next]);
            i = next;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_sensitive_key_substring_match() {
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("chat_model.api_key"));
        assert!(is_sensitive_key("AccessToken"));
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("base_url"));
    }

    #[test]
    fn mask_sensitive_json_recursive() {
        let mut value: serde_json::Value = serde_json::json!({
            "username": "alice",
            "password": "p@ss",
            "chat_model": {
                "api_key": "sk-123",
                "base_url": "https://x",
                "items": [{"token": "t"}]
            }
        });
        mask_sensitive_json(&mut value);
        assert_eq!(value["username"], "alice");
        assert_eq!(value["password"], "***");
        assert_eq!(value["chat_model"]["api_key"], "***");
        assert_eq!(value["chat_model"]["base_url"], "https://x");
        assert_eq!(value["chat_model"]["items"][0]["token"], "***");
    }

    #[test]
    fn mask_sensitive_text_kv_patterns() {
        // key=value
        assert_eq!(
            mask_sensitive_text("connecting with api_key=sk-123 ok"),
            "connecting with api_key=*** ok"
        );
        // key: value（带空格，分隔符与空格保留）
        assert_eq!(
            mask_sensitive_text("password: hunter2, retry"),
            "password: ***, retry"
        );
        // JSON 字符串形态
        assert_eq!(
            mask_sensitive_text(r#"{"api_key":"sk-123","n":1}"#),
            r#"{"api_key":***,"n":1}"#
        );
        // Bearer 双 token
        assert_eq!(
            mask_sensitive_text("Authorization: Bearer abc.def.ghi next"),
            "Authorization: *** next"
        );
        // 非敏感键不受影响
        assert_eq!(
            mask_sensitive_text("count=42 username=alice"),
            "count=42 username=alice"
        );
        // 普通单词中的 token 子串不受影响（无分隔符不命中）
        assert_eq!(
            mask_sensitive_text("tokenize the input"),
            "tokenize the input"
        );
        // 中文等多字节字符不受影响
        assert_eq!(
            mask_sensitive_text("创建 Agent：api_key=sk-1 完成"),
            "创建 Agent：api_key=*** 完成"
        );
        // secret 鍵
        assert_eq!(
            mask_sensitive_text("client_secret = very-secret-value;"),
            "client_secret = ***;"
        );
    }

    #[test]
    fn mask_sensitive_text_flag_form_redacts_value() {
        // CLI flag 形态 `--key value`：key 前为 `-`，空格分隔，脱敏其后 value
        assert_eq!(
            mask_sensitive_text("git push --token secret123"),
            "git push --token ***"
        );
        assert_eq!(
            mask_sensitive_text("curl -H 'Authorization: Bearer x' --password hunter2 done"),
            "curl -H 'Authorization: ***' --password *** done"
        );
        // 普通文本里的敏感子串（key 前非 `-`）不触发 flag 形态，避免误伤
        assert_eq!(
            mask_sensitive_text("my token is abc123 and secret stays"),
            "my token is abc123 and secret stays"
        );
        // 仅 `--key` 无可脱敏 value 时不破坏原文
        assert_eq!(
            mask_sensitive_text("run --token --verbose"),
            "run --token --verbose"
        );
    }

    #[test]
    fn mask_sensitive_json_recurses_into_string_values() {
        // 非敏感键承载的裸字符串值（如 shell 命令参数）也走文本脱敏
        let mut value: serde_json::Value = serde_json::json!({
            "command": "git push --token secret123",
            "env": { "password": "hunter2" },
            "note": "my token is abc"
        });
        mask_sensitive_json(&mut value);
        assert_eq!(value["command"], "git push --token ***");
        assert_eq!(value["env"]["password"], "***");
        // 无 `-` 前缀的敏感子串不被 flag 形态误伤
        assert_eq!(value["note"], "my token is abc");
    }
}
