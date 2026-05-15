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
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;

use crate::pkg::RequestContext;

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
    let filter_layer =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 如果启用文件日志，输出到配置的目录
    if config.logging.enable_file_log {
        let logs_dir = config.log_dir();
        // 创建日志目录（如果不存在，load_config 已经创建过，但保险起见再检查一次）
        if !logs_dir.exists() {
            std::fs::create_dir_all(&logs_dir)
                .expect(&format!("Failed to create logs directory at {:?}", logs_dir));
        }

        // 自动清理旧日志
        if config.logging.retention_days > 0 {
            let retention_period = Duration::from_secs(config.logging.retention_days as u64 * 86400);
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
        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = now.duration_since(modified) {
                    if elapsed > retention {
                        let _ = fs::remove_file(&path);
                        deleted_count += 1;
                    }
                }
            }
        }
    }

    Ok(deleted_count)
}

/// 创建带完整上下文信息的 span（供日志宏使用）
///
/// 包含 RequestContext 中的所有字段：
/// - log_id: 日志追踪 ID
/// - user_id: 用户 ID
/// - username: 用户名
/// - organization_id: 组织 ID
/// - agent_id: Agent ID
/// - task_id: 任务 ID
/// - project_id: 项目 ID
/// - operation: 当前操作名称
#[doc(hidden)]
pub fn create_span(operation: &str, ctx: &RequestContext) -> tracing::Span {
    tracing::info_span!(
        "request",
        log_id = %ctx.log_id,
        user_id = %ctx.user_id.as_deref().unwrap_or(""),
        username = %ctx.username.as_deref().unwrap_or(""),
        organization_id = %ctx.organization_id.as_deref().unwrap_or(""),
        agent_id = %ctx.agent_id.as_deref().unwrap_or(""),
        task_id = %ctx.task_id.as_deref().unwrap_or(""),
        project_id = %ctx.project_id.as_deref().unwrap_or(""),
        operation = %operation
    )
}

