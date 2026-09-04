//! 任务/项目/Agent/工具调用 状态映射

use common::api::ToolCallStatusDto;
use common::enums::skill::SkillAuthorType;

/// Agent 生命周期状态中文文案（0=已删除, 1=面试中, 2=待入职, 3=已入职, 4=已离职, 5=待离职）
/// 单一事实源：避免各页面散写生命周期映射。
pub fn agent_lifecycle_text(status: i32) -> &'static str {
    match status {
        0 => "已删除",
        1 => "面试中",
        2 => "待入职",
        3 => "已入职",
        4 => "已离职",
        5 => "待离职",
        _ => "未知",
    }
}

/// Agent 生命周期状态徽章 class（单一事实源）。
/// 与 `agent_runtime_badge` 对齐：统一走 `badge hud-badge badge-sm`，尺寸一致。
pub fn agent_lifecycle_badge(status: i32) -> &'static str {
    match status {
        0 => "badge hud-badge badge-sm badge-error",
        1 => "badge hud-badge badge-sm badge-warning",
        2 => "badge hud-badge badge-sm badge-info",
        3 => "badge hud-badge badge-sm badge-success",
        4 => "badge hud-badge badge-sm badge-neutral",
        5 => "badge hud-badge badge-sm badge-ghost",
        _ => "badge hud-badge badge-sm badge-ghost",
    }
}

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
        0 => "badge hud-badge badge-sm badge-error",
        1 => "badge hud-badge badge-sm badge-warning",
        2 => "badge hud-badge badge-sm badge-info",
        3 => "badge hud-badge badge-sm badge-primary",
        4 => "badge hud-badge badge-sm badge-success",
        5 => "badge hud-badge badge-sm badge-neutral",
        _ => "badge hud-badge badge-sm badge-neutral",
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
        0 => "badge hud-badge badge-sm badge-error",
        1 => "badge hud-badge badge-sm badge-info",
        2 => "badge hud-badge badge-sm badge-warning",
        3 => "badge hud-badge badge-sm badge-primary",
        4 => "badge hud-badge badge-sm badge-success",
        5 => "badge hud-badge badge-sm badge-neutral",
        _ => "badge hud-badge badge-sm badge-neutral",
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

/// 配置维度徽章（HUD 玻璃徽章，单一事实源）。
///
/// 用于在设置页等场景标注某配置项的归属维度：
/// - 服务级（默认）：跟随当前访问环境（如后端 API 地址，存 localStorage）
/// - 组织级：跟随当前组织、全局生效（如消息向量索引开关）
///
/// 与状态徽章共用 `badge hud-badge badge-sm` 基底，保持尺寸与质感一致，
/// 避免页面散写 `badge` 颜色类。
pub fn config_dimension_badge(level: &str) -> &'static str {
    match level {
        "org" => "badge hud-badge badge-sm badge-primary",
        _ => "badge hud-badge badge-sm badge-ghost", // 服务级为默认分支
    }
}

/// 授权 / 登录状态徽章（单一事实源）。
///
/// 取代原先散落在 finance 各页面、且一律用 `badge-ghost`（几乎不可见）的写法：
/// - 已登录 / 已授权 → success(绿)
/// - 未登录 → warning(琥珀)
/// - 未授权 / 用户未授权 → error(红)
pub fn auth_state_badge(state: &str) -> &'static str {
    match state {
        "已登录" | "已授权" | "用户已授权" => "badge hud-badge badge-sm badge-success",
        "未登录" => "badge hud-badge badge-sm badge-warning",
        "未授权" | "用户未授权" => "badge hud-badge badge-sm badge-error",
        _ => "badge hud-badge badge-sm badge-ghost",
    }
}

/// 优先级徽章（实心色阶，清晰可读）。
///
/// 配色与看板（kanban_canvas）保持一致：
/// 0=默认(中性) / 1-5=警示(琥珀) / >5=紧急(红)。
pub fn priority_badge(priority: i32) -> &'static str {
    match priority {
        p if p > 5 => "badge hud-badge badge-sm badge-error",
        p if p > 0 => "badge hud-badge badge-sm badge-warning",
        _ => "badge hud-badge badge-sm badge-neutral",
    }
}

/// Agent 运行时状态徽章（单一事实源，i32 版）。
///
/// 语义（对齐后端 `AgentRuntimeState`）：
/// 空闲(0)=success(绿) / 休息中(1)=neutral(灰) / 忙碌(2)=warning(琥珀，≠异常) / 未知=ghost。
/// 取代原先分散在 workspace / chat 中彼此冲突的三套映射。
pub fn agent_runtime_badge(state: i32) -> &'static str {
    match state {
        0 => "badge hud-badge badge-sm badge-success",
        1 => "badge hud-badge badge-sm badge-neutral",
        2 => "badge hud-badge badge-sm badge-warning",
        _ => "badge hud-badge badge-sm badge-ghost",
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
        "idle" => "badge hud-badge badge-sm badge-success",
        "resting" => "badge hud-badge badge-sm badge-neutral",
        "busy" => "badge hud-badge badge-sm badge-warning",
        _ => "badge hud-badge badge-sm badge-ghost",
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

/// 工具调用状态中文文案（单一事实源）
pub fn tool_call_status_text(status: ToolCallStatusDto) -> &'static str {
    match status {
        ToolCallStatusDto::Started => "执行中",
        ToolCallStatusDto::Completed => "已完成",
        ToolCallStatusDto::Failed => "失败",
    }
}

/// 工具调用状态徽章 class（单一事实源）
///
/// 与 `task_status_badge` / `project_status_badge` 对齐，统一走
/// `badge hud-badge badge-sm badge-xxx`，尺寸与语义一致。
pub fn tool_call_status_badge(status: ToolCallStatusDto) -> &'static str {
    match status {
        ToolCallStatusDto::Started => "badge hud-badge badge-sm badge-info",
        ToolCallStatusDto::Completed => "badge hud-badge badge-sm badge-success",
        ToolCallStatusDto::Failed => "badge hud-badge badge-sm badge-error",
    }
}

/// 技能「作者类型」徽章 class（单一事实源，HUD 风格对齐）。
///
/// 设计准则：**作者类型是「属性/类别」而非状态**，因此走中性 `orz-tag` chip
/// （对齐 tag_chip 的基底 class），而非彩色 hud-badge 玻璃徽章，以保证：
/// - 和状态徽章（Expired / Published / Draft）形成视觉层级差；
/// - 和角色/能力/标签等其他属性徽章在全站点保持同一视觉语言。
///
/// 语义区分通过颜色填充而非形状差异：
/// - Agent = 青蓝 orz-tag + badge-info（系统/自动产物）
/// - User  = 中性 orz-tag（人类作者，默认）
pub fn skill_author_type_badge(author_type: SkillAuthorType) -> &'static str {
    match author_type {
        SkillAuthorType::Agent => "badge orz-tag badge-sm badge-info",
        SkillAuthorType::User => "badge orz-tag badge-sm",
    }
}

/// 技能「作者类型」中文文案（单一事实源）。
pub fn skill_author_type_text(author_type: SkillAuthorType) -> &'static str {
    match author_type {
        SkillAuthorType::Agent => "Agent 创作",
        SkillAuthorType::User => "用户创作",
    }
}

/// 对任意长 ID（UUID / ulid / object_id）截取短展示形式（前 6 后 4），
/// 用于作者 ID 这类"一眼识别但不占空间"的场景；ID 不足 10 位直接原样返回。
pub fn short_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    let n = chars.len();
    if n <= 10 {
        return id.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[n - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// 组织组网连接状态文案（对齐后端 `OrganizationLinkStatus`：1=Active 0=Revoked）。
pub fn org_link_status_text(status: i32) -> &'static str {
    match status {
        1 => "已建联",
        0 => "已断联",
        _ => "未知",
    }
}

/// 组织组网连接状态徽章（单一事实源）。
///
/// 已建联(1)=success(绿) / 已断联(0)=error(红) / 未知=ghost。
pub fn org_link_status_badge(status: i32) -> &'static str {
    match status {
        1 => "badge hud-badge badge-sm badge-success",
        0 => "badge hud-badge badge-sm badge-error",
        _ => "badge hud-badge badge-sm badge-ghost",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_status_variants() {
        assert_eq!(tool_call_status_text(ToolCallStatusDto::Started), "执行中");
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Started),
            "badge hud-badge badge-sm badge-info"
        );
        assert_eq!(
            tool_call_status_text(ToolCallStatusDto::Completed),
            "已完成"
        );
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Completed),
            "badge hud-badge badge-sm badge-success"
        );
        assert_eq!(tool_call_status_text(ToolCallStatusDto::Failed), "失败");
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Failed),
            "badge hud-badge badge-sm badge-error"
        );
    }

    #[test]
    fn org_link_status_variants() {
        assert_eq!(org_link_status_text(1), "已建联");
        assert_eq!(
            org_link_status_badge(1),
            "badge hud-badge badge-sm badge-success"
        );
        assert_eq!(org_link_status_text(0), "已断联");
        assert_eq!(
            org_link_status_badge(0),
            "badge hud-badge badge-sm badge-error"
        );
    }
}
