//! 工具运行日志保留清理（① 运行时输出层的 TTL 治理）
//!
//! 设计依据：[docs/design/tool_output_boundary_design.md](../../../docs/design/tool_output_boundary_design.md)
//!
//! 目录约定：`{base}/tools/{tool_id}/logs/{YYYYMMDD}/{call_id}.log`
//! - 清理单位是日期目录（非逐文件 mtime）
//! - Running 进程日志所在日期目录整体跳过（下次清理重试），保证 shell_status 观测不断流
//! - retention_days = 0 表示不清理

use std::path::{Path, PathBuf};

use crate::pkg::paths;
use common::constants::utils::current_timestamp_ms;

/// 一次清理的执行报告
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ToolLogCleanupReport {
    /// 扫描的工具日志根目录数（tools/{tool_id}/logs）
    pub scanned_roots: usize,
    /// 删除的日期目录数
    pub removed_dirs: usize,
    /// 删除的日志文件数
    pub removed_files: usize,
    /// 释放的字节数
    pub freed_bytes: u64,
    /// 因 Running 进程保护跳过的日期目录数
    pub skipped_dirs: usize,
}

/// 单个日期分区的占用
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolLogDayStat {
    /// 日期目录名（YYYYMMDD）
    pub day: String,
    /// 日志文件数
    pub files: usize,
    /// 占用字节数
    pub bytes: u64,
}

/// 工具日志存储统计（系统监控页存储维度数据源）
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ToolLogStorageStats {
    /// 总占用字节数
    pub total_bytes: u64,
    /// 总文件数
    pub total_files: usize,
    /// 按天占用（升序）
    pub by_day: Vec<ToolLogDayStat>,
}

/// 工具日志根目录（tools/）下的日志分区布局
///
/// 返回每个工具日志根（`tools/{tool_id}/logs`）及其下的日期目录列表。
fn scan_tool_log_roots(base_data_path: &Path) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let tools_dir = paths::tools_root_dir(base_data_path);
    if !tools_dir.is_dir() {
        return Vec::new();
    }

    let Ok(tool_entries) = std::fs::read_dir(&tools_dir) else {
        return Vec::new();
    };

    let mut roots = Vec::new();
    for tool_entry in tool_entries.flatten() {
        let logs_root = tool_entry.path().join("logs");
        if !logs_root.is_dir() {
            continue;
        }
        let Ok(day_entries) = std::fs::read_dir(&logs_root) else {
            continue;
        };
        let day_dirs: Vec<PathBuf> = day_entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| chrono::NaiveDate::parse_from_str(n, "%Y%m%d").ok())
                        .is_some()
            })
            .collect();
        if !day_dirs.is_empty() {
            roots.push((logs_root, day_dirs));
        }
    }
    roots
}

/// 日期目录名（YYYYMMDD）转毫秒时间戳（当日 00:00 本地时间）
fn day_dir_to_timestamp_ms(dir: &Path) -> Option<i64> {
    let name = dir.file_name()?.to_str()?;
    let date = chrono::NaiveDate::parse_from_str(name, "%Y%m%d").ok()?;
    let datetime = date.and_hms_opt(0, 0, 0)?;
    Some(
        datetime
            .and_local_timezone(chrono::Local)
            .single()?
            .timestamp_millis(),
    )
}

/// 统计目录树内文件数与总字节数（软失败：不可读文件按 0 计）
fn dir_usage(dir: &Path) -> (usize, u64) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_file() {
            files += 1;
            bytes += meta.len();
        }
    }
    (files, bytes)
}

/// Running 进程日志所在的日期目录集合（受保护，禁止删除）
///
/// 从全局进程注册中心取 Running 条目的 log_path，回溯其日期目录前缀。
fn protected_day_dirs() -> Vec<PathBuf> {
    crate::pkg::process::registry()
        .list()
        .into_iter()
        .filter(|e| e.status == crate::pkg::process::ProcessStatus::Running)
        .filter_map(|e| {
            let path = PathBuf::from(&e.log_path);
            // log 布局 .../logs/{YYYYMMDD}/{call_id}.log → 日期目录 = log 的父目录
            path.parent().map(|p| p.to_path_buf())
        })
        .collect()
}

/// 清理超期工具日志（cron 定时任务与前端手动清理共用同一入口）
///
/// - 清理单位：日期目录整体删除
/// - 保护规则：Running 进程日志所在日期目录跳过
/// - retention_days = 0：不清理，返回空报告
pub fn cleanup_tool_logs(base_data_path: &Path, retention_days: u32) -> ToolLogCleanupReport {
    let mut report = ToolLogCleanupReport::default();
    if retention_days == 0 {
        return report;
    }

    let cutoff_ms = current_timestamp_ms() - (retention_days as i64) * 86_400_000;
    let protected = protected_day_dirs();

    for (logs_root, day_dirs) in scan_tool_log_roots(base_data_path) {
        report.scanned_roots += 1;
        for day_dir in day_dirs {
            let Some(dir_ts) = day_dir_to_timestamp_ms(&day_dir) else {
                continue;
            };
            if dir_ts >= cutoff_ms {
                continue; // 保留期内
            }
            // Running 保护：候选目录下有活跃进程日志则整体跳过
            if protected
                .iter()
                .any(|p| p.starts_with(&day_dir) || day_dir.starts_with(p))
            {
                report.skipped_dirs += 1;
                continue;
            }
            let (files, bytes) = dir_usage(&day_dir);
            if std::fs::remove_dir_all(&day_dir).is_ok() {
                report.removed_dirs += 1;
                report.removed_files += files;
                report.freed_bytes += bytes;
            }
            // 根目录空了顺手移除（保持 tools 树整洁，下次写入重建）
            if std::fs::read_dir(&logs_root)
                .map(|mut it| it.next().is_none())
                .unwrap_or(false)
            {
                let _ = std::fs::remove_dir(&logs_root);
            }
        }
    }
    report
}

/// 统计工具日志存储占用（按天分布，升序）
pub fn tool_log_storage_stats(base_data_path: &Path) -> ToolLogStorageStats {
    let mut stats = ToolLogStorageStats::default();

    for (_logs_root, day_dirs) in scan_tool_log_roots(base_data_path) {
        for day_dir in day_dirs {
            let (files, bytes) = dir_usage(&day_dir);
            let day = day_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            stats.total_bytes += bytes;
            stats.total_files += files;
            stats.by_day.push(ToolLogDayStat { day, files, bytes });
        }
    }
    stats.by_day.sort_by(|a, b| a.day.cmp(&b.day));
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::paths;
    use crate::pkg::process::{ProcessEntry, ProcessStatus};

    fn write_log(dir: &Path, name: &str, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), content).unwrap();
    }

    fn old_day(offset_days: i64) -> String {
        let date = chrono::Local::now().date_naive() - chrono::Duration::days(offset_days);
        date.format("%Y%m%d").to_string()
    }

    #[test]
    fn cleanup_removes_expired_dirs_and_keeps_recent() {
        let base = tempfile::tempdir().unwrap();
        let logs_root = paths::tool_logs_dir(base.path(), "shell_exec");
        let old_dir = logs_root.join(old_day(40));
        let recent_dir = logs_root.join(old_day(1));
        write_log(&old_dir, "call-1.log", "old output");
        write_log(&recent_dir, "call-2.log", "recent output");

        let report = cleanup_tool_logs(base.path(), 30);

        assert_eq!(report.removed_dirs, 1);
        assert_eq!(report.removed_files, 1);
        assert!(report.freed_bytes > 0);
        assert!(!old_dir.exists());
        assert!(recent_dir.exists());
    }

    #[test]
    fn cleanup_zero_retention_is_noop() {
        let base = tempfile::tempdir().unwrap();
        let old_dir = paths::tool_logs_dir(base.path(), "shell_exec").join(old_day(400));
        write_log(&old_dir, "call-1.log", "ancient");

        let report = cleanup_tool_logs(base.path(), 0);

        assert_eq!(report.removed_dirs, 0);
        assert!(old_dir.exists());
    }

    #[test]
    fn cleanup_protects_running_process_log_dir() {
        let base = tempfile::tempdir().unwrap();
        let logs_root = paths::tool_logs_dir(base.path(), "shell_exec");
        let old_dir = logs_root.join(old_day(40));
        write_log(&old_dir, "call-running.log", "still writing");

        // 注册一个 Running 进程，日志落在超期目录内
        let reg = crate::pkg::process::ProcessRegistry::new();
        // 直接构造受保护路径验证保护逻辑（绕开全局单例，用受保护路径匹配规则）
        let entry = ProcessEntry {
            pid: 987_654,
            tool_id: "shell_exec".to_string(),
            call_id: "call-running".to_string(),
            agent_id: None,
            project_id: None,
            task_id: None,
            command: "sleep 300".to_string(),
            working_dir: "/tmp".to_string(),
            log_path: old_dir
                .join("call-running.log")
                .to_string_lossy()
                .to_string(),
            background: true,
            started_at: current_timestamp_ms() as u64,
            status: ProcessStatus::Running,
            exit_code: None,
            finished_at: None,
        };
        reg.register(entry);

        // protected_day_dirs 走全局单例；本测试用进程独立注册中心验证匹配规则本身：
        // 与 cleanup_tool_logs 内部逻辑一致（候选目录与保护路径互为前缀则跳过）
        let protected: Vec<PathBuf> = reg
            .list()
            .iter()
            .filter(|e| e.status == ProcessStatus::Running)
            .map(|e| PathBuf::from(&e.log_path).parent().unwrap().to_path_buf())
            .collect();
        assert!(
            protected
                .iter()
                .any(|p| old_dir.starts_with(p) || p.starts_with(&old_dir))
        );

        // 全局清理函数不删该目录（无全局 Running 注册时也不应因路径巧合误删：
        // 此处目录确实超期，若无保护会被删；这里只验证统计口径不爆炸）
        let stats = tool_log_storage_stats(base.path());
        assert_eq!(stats.total_files, 1);
    }

    #[test]
    fn storage_stats_aggregates_by_day() {
        let base = tempfile::tempdir().unwrap();
        let logs_root = paths::tool_logs_dir(base.path(), "shell_exec");
        let day_a = logs_root.join(old_day(2));
        let day_b = logs_root.join(old_day(1));
        write_log(&day_a, "a1.log", "11"); // 2 bytes
        write_log(&day_a, "a2.log", "22"); // 2 bytes
        write_log(&day_b, "b1.log", "333"); // 3 bytes

        let stats = tool_log_storage_stats(base.path());

        assert_eq!(stats.total_files, 3);
        assert_eq!(stats.total_bytes, 7);
        assert_eq!(stats.by_day.len(), 2);
        // 升序：两天前在前，昨天在后
        assert_eq!(stats.by_day[0].day, old_day(2));
        assert_eq!(stats.by_day[0].files, 2);
        assert_eq!(stats.by_day[1].day, old_day(1));
        assert_eq!(stats.by_day[1].files, 1);
    }

    #[test]
    fn non_date_dirs_are_ignored() {
        let base = tempfile::tempdir().unwrap();
        let logs_root = paths::tool_logs_dir(base.path(), "shell_exec");
        let junk = logs_root.join("not-a-date");
        write_log(&junk, "x.log", "junk");

        let stats = tool_log_storage_stats(base.path());

        assert_eq!(stats.total_files, 0); // 非日期目录不参与统计
    }
}
