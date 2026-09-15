//! 消息内容构建器
//!
//! 为 Agent Loop Engine 的系统通知场景构建结构化消息内容：
//! - 任务调度通知（场景 2：任务状态变更触发）
//! - 项目跟进通知（场景 3：定时补偿触发）

use common::enums::task::TaskStatus;

/// 构建任务调度通知消息内容（场景 2：任务状态变更触发）
pub fn build_task_dispatch_content(
    task_title: &str,
    new_status: TaskStatus,
    progress: i32,
) -> String {
    format!(
        "📋 任务调度通知\n\
         任务：「{}」状态变更为「{}」（进度：{}%）\n\n\
         作为项目 Owner Agent，请执行以下调度职责：\n\n\
         1. **更新进度**：调用 get_project(with_progress_summary=true) 获取最新进度汇总\n\
         2. **检查计划**：对比 execution_plan，判断当前进展是否符合预期\n\
         3. **调度下一任务**：\n\
            - 检查是否有后续任务的依赖已满足（前置任务已完成）\n\
            - 如有，通过 send_to_agent 通知对应 Agent 开始执行\n\
            - 如无后续任务，检查是否所有任务已完成 → 更新项目状态为 Completed\n\
         4. **面向用户的收尾**：你的最终回复会自动送达项目归属用户，**无需**再调用 send_message 重复通知；\n\
            请在最终回复中写清：本次调度做了什么、项目当前状态与进度、下一步计划；\n\
            如存在需要用户决策的事项（阻塞风险 / 计划变更），明确指出需要用户做什么",
        task_title,
        task_status_label(new_status),
        progress,
    )
}

/// 构建项目跟进通知消息内容（场景 3：定时补偿触发）
pub fn build_project_followup_content(project_name: &str) -> String {
    format!(
        "📊 项目进度定期检查\n\
         项目：「{}」\n\n\
         系统定时触发了项目跟进检查，请执行以下检查：\n\n\
         1. **获取进度**：调用 get_project(with_progress_summary=true) 获取整体进度\n\
         2. **识别阻塞**：\n\
            - 检查 InProgress 任务是否有长时间无更新的（可能卡住了）\n\
            - 检查 Pending 任务是否因依赖阻塞无法启动\n\
         3. **对比计划**：对照 execution_plan，判断当前阶段是否正常推进\n\
         4. **采取行动**：\n\
            - 任务 / 项目状态有明确变化（任务完成 / 阻塞 / 取消、进度或状态流转、里程碑达成）→ **必须** send_message 通知项目归属用户，写清「变化内容 + 下一步」\n\
            - 阻塞任务 → 分析原因，调整分配或通知用户\n\
            - 全部完成 → 更新项目状态为 Completed，并按上一条通知用户\n\
            - 需要调整计划 → 更新 execution_plan\n\
            - 无状态变化、进展正常 → 不必通知（避免每小时重复打扰）\n\n\
         注意：本次唤醒的 Final 文本不会自动送达用户（定时巡检每小时触发，避免打扰）。\n\
         需要向用户同步进展或请求决策时，请调用 send_message，目标用户见【项目上下文】的项目归属用户。",
        project_name,
    )
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Cancelled => "已取消",
        TaskStatus::Pending => "待开始",
        TaskStatus::InProgress => "进行中",
        TaskStatus::Completed => "已完成",
        TaskStatus::Archived => "已归档",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_task_dispatch_content() {
        let content = build_task_dispatch_content("搭建脚手架", TaskStatus::Completed, 100);
        assert!(content.contains("任务调度通知"));
        assert!(content.contains("搭建脚手架"));
        assert!(content.contains("已完成"));
        assert!(content.contains("get_project"));
        // 该场景 Final 由消费者兜底投递，正文需说明「最终回复即送达」以免重复 send_message
        assert!(content.contains("自动送达"));
        assert!(content.contains("无需"));
    }

    #[test]
    fn test_build_project_followup_content() {
        let content = build_project_followup_content("AI 助手开发");
        assert!(content.contains("项目进度定期检查"));
        assert!(content.contains("AI 助手开发"));
        assert!(content.contains("识别阻塞"));
        // 巡检场景不兜底投递 Final，必须显式告知 Agent 走 send_message
        assert!(content.contains("send_message"));
        assert!(content.contains("不会自动送达用户"));
        // 状态有明确变化 = 确定性通知条件（软要求改硬）
        assert!(content.contains("必须"));
        assert!(content.contains("状态有明确变化"));
        // 无变化则不通知，避免每小时噪音
        assert!(content.contains("无状态变化"));
    }
}
