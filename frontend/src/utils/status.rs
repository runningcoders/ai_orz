//! 任务/项目状态映射

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
