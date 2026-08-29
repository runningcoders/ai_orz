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
        0 => "badge badge-sm badge-error",
        1 => "badge badge-sm badge-warning",
        2 => "badge badge-sm badge-info",
        3 => "badge badge-sm badge-primary",
        4 => "badge badge-sm badge-success",
        5 => "badge badge-sm badge-neutral",
        _ => "badge badge-sm badge-neutral",
    }
}

/// 进度条 HUD 色调（0-25=warning, 26-50=primary, 51-75=accent, 76-100=success）
/// 供 `HudProgress` 的 `tone` 复用。
pub fn progress_tone(progress: i32) -> &'static str {
    match progress {
        0..=25 => "warning",
        26..=50 => "primary",
        51..=75 => "accent",
        76..=100 => "success",
        _ => "primary",
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
        0 => "badge badge-sm badge-error",
        1 => "badge badge-sm badge-info",
        2 => "badge badge-sm badge-warning",
        3 => "badge badge-sm badge-primary",
        4 => "badge badge-sm badge-success",
        5 => "badge badge-sm badge-neutral",
        _ => "badge badge-sm badge-neutral",
    }
}

/// 自定义标签（tag / chip）统一样式：中性 soft chip，随主题自适应。
///
/// 用于项目标签、Agent 角色/能力、工具标签等用户自定义文本，
/// 与状态徽章（filled 实底）形成「状态 vs 属性」的视觉层级区分，
/// 避免之前 `badge badge-outline`（透明割裂）/ `badge badge-ghost`（近乎隐身）的混排问题。
pub fn tag_chip() -> &'static str {
    "badge orz-tag badge-sm"
}

/// 授权 / 登录状态徽章（单一事实源）。
///
/// 取代原先散落在 finance 各页面、且一律用 `badge-ghost`（几乎不可见）的写法：
/// - 已登录 / 已授权 → success(绿)
/// - 未登录 → warning(琥珀)
/// - 未授权 / 用户未授权 → error(红)
pub fn auth_state_badge(state: &str) -> &'static str {
    match state {
        "已登录" | "已授权" | "用户已授权" => "badge badge-sm badge-success",
        "未登录" => "badge badge-sm badge-warning",
        "未授权" | "用户未授权" => "badge badge-sm badge-error",
        _ => "badge badge-sm badge-ghost",
    }
}

/// 优先级徽章（实心色阶，清晰可读）。
///
/// 配色与看板（kanban_canvas）保持一致：
/// 0=默认(中性) / 1-5=警示(琥珀) / >5=紧急(红)。
pub fn priority_badge(priority: i32) -> &'static str {
    match priority {
        p if p > 5 => "badge badge-sm badge-error",
        p if p > 0 => "badge badge-sm badge-warning",
        _ => "badge badge-sm badge-neutral",
    }
}

/// Agent 运行时状态徽章（单一事实源，i32 版）。
///
/// 语义（对齐后端 `AgentRuntimeState`）：
/// 空闲(0)=success(绿) / 休息中(1)=neutral(灰) / 忙碌(2)=warning(琥珀，≠异常) / 未知=ghost。
/// 取代原先分散在 workspace / chat 中彼此冲突的三套映射。
pub fn agent_runtime_badge(state: i32) -> &'static str {
    match state {
        0 => "badge badge-sm badge-success",
        1 => "badge badge-sm badge-neutral",
        2 => "badge badge-sm badge-warning",
        _ => "badge badge-sm badge-ghost",
    }
}

/// Agent 运行时状态徽章（单一事实源，字符串版，用于运行时 Agent 列表）。
///
/// 与 [`agent_runtime_badge`]（i32 版）互为镜像，统一的字符串态映射来源。
/// 当前调用方（HUD 顶部状态条走计数聚合）尚未直接消费，保留为统一事实源以待
/// 运行时 Agent 列表面板接入，故显式 allow。
#[allow(dead_code)]
pub fn agent_runtime_badge_str(state: &str) -> &'static str {
    match state {
        "idle" => "badge badge-sm badge-success",
        "resting" => "badge badge-sm badge-neutral",
        "busy" => "badge badge-sm badge-warning",
        _ => "badge badge-sm badge-ghost",
    }
}

/// 任务状态对应的 HUD 风格颜色（hex，适配深色背景）
///
/// 颜色语义与 `task_status_badge` 对齐，但用更鲜艳的 hex 值适配 HUD 深色背景：
/// - 0 已取消：红色 #ef4444
/// - 1 待审核：橙黄 #f59e0b
/// - 2 待处理：蓝色 #3b82f6
/// - 3 进行中：HUD 主色橙 #fa520f
/// - 4 已完成：绿色 #10b981
/// - 5 已归档：灰色 #6b7280
pub fn task_status_color(status: i32) -> &'static str {
    match status {
        0 => "#ef4444",
        1 => "#f59e0b",
        2 => "#3b82f6",
        3 => "#fa520f",
        4 => "#10b981",
        5 => "#6b7280",
        _ => "#6b7280",
    }
}
