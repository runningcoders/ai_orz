//! 消息内容构建器
//!
//! 为 Agent Loop Engine 的系统通知场景构建结构化消息内容：
//! - 任务调度通知（场景 2：任务状态变更触发）
//! - 项目跟进通知（场景 3：定时补偿触发）

use common::enums::task::TaskStatus;
use common::models::ProjectProgressSummary;

/// 构建任务调度通知消息内容（场景 2：任务状态变更触发）
///
/// **这是一条机械性兜底通知，不是「第一次告知」**：同一次任务完成可能已经通过
/// 其他路径（Agent 自主汇报、上一轮编排）驱动过进展，触发时也没有任何「是否已处理」
/// 的可靠判据 —— 所以正文必须**先自证再行动**：让 Agent 用 `get_project` 核对现状，
/// 已处理就收住，不要重复派发、重复催办、重复通知用户。
///
/// 这里的取舍是刻意的：宁可让 Owner 多跑一轮核对（代价 = 一次查询 + 一小段回复），
/// 也不能因为「可能重复」而不发（代价 = 项目永久停在原地，且无任何报错）。
pub fn build_task_dispatch_content(
    task_title: &str,
    new_status: TaskStatus,
    progress: i32,
) -> String {
    format!(
        "📋 任务调度通知（兜底触发）\n\
         任务：「{}」状态变更为「{}」（进度：{}%）\n\n\
         ⚠️ **这是兜底通知**：本变更可能已通过其他路径驱动过一轮编排，本通知不保证是首次告知。\n\
         请先用 get_project(with_progress_summary=true) 核对现状，若下列事项**已经完成**，\n\
         说明本轮无需新增动作 —— 不要重复派发任务、不要重复催办、不要重复通知用户，\n\
         简短说明「已是最新状态」即可结束。\n\n\
         作为项目 Owner Agent，请执行以下调度职责（仅在尚未完成时）：\n\n\
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
///
/// `progress` 为消费侧预取的进度汇总（省去 Agent 的一跳查询，也让通知更具体）；
/// 传 `None` 时不注入快照块（保持正文与历史一致）。
///
/// 触发器以**项目归属用户身份中继**本通知（from_role=User），Agent 的 Final
/// 自动回复会送达该用户——因此正文把「通知用户」收敛为「最终回复的写法要求」，
/// 不再要求调 send_message（那会造成双重通知）。
///
/// **推进优先原则**：巡检的目的不是产出一份进度汇报，而是推进进度。正文必须
/// 明确区分「无需用户确认、立即执行的动作」（按既定 execution_plan 调度 / 催办 /
/// 调整计划 / 关闭项目）与「需用户决策、只上报选项的动作」（改方向 / 改范围），
/// 否则 Agent 会保守地退化为「只汇报现状不推进」。
pub fn build_project_followup_content(
    project_name: &str,
    progress: Option<&ProjectProgressSummary>,
) -> String {
    let progress_snapshot = match progress {
        Some(p) => format!(
            "\n当前进度快照：共 {} 个任务（已完成 {} / 进行中 {} / 待启动 {} / 已取消 {}），整体进度 {}%。\n",
            p.total_tasks, p.completed, p.in_progress, p.pending, p.cancelled, p.overall_percent
        ),
        None => String::new(),
    };
    format!(
        "📊 项目进度定期检查\n\
         项目：「{}」\n{}\n\
         系统定时触发了项目跟进检查（以项目归属用户名义转发给你）。巡检的目的不是写\n\
         一份进度汇报，而是推进进度：**能自主推进的立即推进，再汇报你做了什么**。\n\n\
         1. **获取进度**：调用 get_project(with_progress_summary=true, with_task_graph=true) 获取整体进度与任务依赖图\n\
         2. **识别可推进点**：\n\
            - Pending 任务：dependencies 前置已全部 Completed → 可立即启动\n\
            - InProgress 任务：modified_at 长时间无更新 / progress 长期不动 → 可能卡住\n\
            - 对照 execution_plan，判断当前阶段是否落后于计划\n\
         3. **立即推进（无需用户确认，直接执行）**：凡是不改变项目方向 / 范围 / 交付承诺、\n\
            只是按既定 execution_plan 往前推的动作，都不要请示，直接做：\n\
            - 可启动的 Pending 任务 → send_task_assignment_message 通知其 Task Owner 启动；\n\
              Task Owner 是你自己 → 直接开工（update_task_status(InProgress) 后按执行循环推进）\n\
            - 停滞的 InProgress 任务 → send_task_assignment_message 催办，要求更新进度或说明阻塞原因\n\
            - 计划需调整 → update_project(execution_plan=修订版)\n\
            - 全部任务完成 → update_project_status(Completed)\n\
         4. **需要用户决策的才上报**：改变方向 / 范围的动作（新增需求、取消或删除任务、\n\
            里程碑延期、方案分歧）不要擅自执行，在最终回复中写清「阻塞点 + 你的建议选项」，\n\
            等用户决策\n\
         5. **汇报本轮动作**：最终回复写清本轮推进了什么（调度 / 催办 / 调整了哪些任务、\n\
            各任务当前状态与进度）、下一步；仅当确实不存在任何可推进点且无变化时，才回\n\
            一行简短确认（巡检每小时触发，避免重复打扰）\n\n\
         注意：你的最终回复会自动送达项目归属用户，**无需**调用 send_message 重复通知用户；\n\
         send_task_assignment_message / send_message 仅用于通知其他 Agent 的场景。",
        project_name, progress_snapshot,
    )
}

/// 构建项目收尾通知消息内容（场景 3 变体：全部任务已终态、项目仍 InProgress）
///
/// 与 [`build_project_followup_content`] 的区别：此时**没有可推进的任务**，
/// 「催办进度」语义不再适用——项目大概率卡在收尾环节（状态流转到 Completed +
/// 最终汇报）。正文改为「二选一」指令：目标已达成则关闭项目并汇报；确有后续
/// 工作则创建新任务继续推进。避免把长周期项目（阶段性建任务）误杀成「必须关」，
/// 也避免 Agent 对着一个 100% 的进度快照困惑地乱建任务。
///
/// 消息类型仍为 `ProjectFollowupNotification`：②③④⑤ 所有防打扰门禁原样生效。
pub fn build_project_finalize_content(
    project_name: &str,
    progress: &ProjectProgressSummary,
) -> String {
    format!(
        "🏁 项目收尾检查\n\
         项目：「{}」\n\
         当前进度快照：共 {} 个任务（已完成 {} / 已取消 {}），整体进度 {}%。\n\n\
         巡检发现：项目仍处于「进行中」，但已**没有任何可推进的任务**（无待启动 / 进行中 /\n\
         阻塞）。这通常意味着项目卡在了收尾环节。请二选一处理：\n\n\
         1. **项目目标已达成（多数情况）**：核对任务产出与 execution_plan 后，直接执行\n\
            update_project_status(Completed) 完成项目，并在最终回复中输出最终汇报\n\
            （做了什么、交付了什么、遗留事项与建议）\n\
         2. **确有后续工作（如长周期项目的下一阶段）**：创建新任务（create_task）并给出\n\
            明确的标题 / 描述 / 优先级，让项目回到有活可干的状态；不要只为应付检查而\n\
            创建占位任务\n\n\
         注意：你的最终回复会自动送达项目归属用户，**无需**调用 send_message 重复通知用户。",
        project_name,
        progress.total_tasks,
        progress.completed,
        progress.cancelled,
        progress.overall_percent,
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
        // 机械性兜底通知：必须让 Agent 先自证「是否已处理」，否则它会重复派发/重复通知
        assert!(content.contains("兜底通知"), "正文必须自证这是兜底触发");
        assert!(
            content.contains("不要重复派发") && content.contains("不要重复通知"),
            "正文必须明确「已处理则不要重复动作」"
        );
    }

    #[test]
    fn test_build_project_followup_content() {
        let content = build_project_followup_content("AI 助手开发", None);
        assert!(content.contains("项目进度定期检查"));
        assert!(content.contains("AI 助手开发"));
        assert!(content.contains("识别可推进点"));
        // 推进优先：明确要求无需用户确认立即推进，而不是只汇报现状
        assert!(content.contains("立即推进"));
        assert!(content.contains("无需用户确认"));
        assert!(content.contains("send_task_assignment_message"));
        // 巡检以归属用户身份中继，Final 自动送达 → 无需（也不该）再 send_message
        assert!(content.contains("自动送达"));
        assert!(content.contains("无需"));
        // 需要用户决策的动作只上报「阻塞点 + 建议选项」，不擅自执行
        assert!(content.contains("需要用户决策"));
        // 无任何可推进点才回一行确认，避免每小时噪音
        assert!(content.contains("一行简短确认"));
        // 未传进度时不注入快照块
        assert!(!content.contains("当前进度快照"));
    }

    /// 传入进度汇总时正文追加「当前进度快照」块（消费侧预取，省去 Agent 一跳查询）
    #[test]
    fn test_build_project_followup_content_with_progress_snapshot() {
        let progress = ProjectProgressSummary {
            total_tasks: 5,
            completed: 2,
            in_progress: 1,
            pending: 1,
            blocked: 0,
            cancelled: 1,
            overall_percent: 42,
        };
        let content = build_project_followup_content("AI 助手开发", Some(&progress));
        assert!(content.contains("当前进度快照"));
        assert!(content.contains("共 5 个任务"));
        assert!(content.contains("已完成 2"));
        assert!(content.contains("进行中 1"));
        assert!(content.contains("待启动 1"));
        assert!(content.contains("已取消 1"));
        assert!(content.contains("整体进度 42%"));
    }

    /// 收尾文案：全部任务终态时的跟进通知应切换为「二选一收尾」语义
    #[test]
    fn test_build_project_finalize_content() {
        let progress = ProjectProgressSummary {
            total_tasks: 3,
            completed: 2,
            in_progress: 0,
            pending: 0,
            blocked: 0,
            cancelled: 1,
            overall_percent: 100,
        };
        let content = build_project_finalize_content("数据迁移项目", &progress);
        assert!(content.contains("项目收尾检查"));
        assert!(content.contains("数据迁移项目"));
        assert!(content.contains("共 3 个任务"));
        assert!(content.contains("已完成 2"));
        assert!(content.contains("已取消 1"));
        // 二选一指令：默认关闭项目 + 汇报；确有后续工作则建新任务
        assert!(content.contains("update_project_status(Completed)"));
        assert!(content.contains("create_task"));
        // 巡检以归属用户身份中继，Final 自动送达 → 无需再 send_message
        assert!(content.contains("自动送达"));
        assert!(content.contains("无需"));
    }

    /// `all_tasks_terminal` 判定边界：有活跃任务/空项目不算，终态组合（含取消）算
    #[test]
    fn test_progress_summary_all_tasks_terminal() {
        let mk =
            |total: usize, completed: usize, in_prog: usize, pending: usize, cancelled: usize| {
                ProjectProgressSummary {
                    total_tasks: total,
                    completed,
                    in_progress: in_prog,
                    pending,
                    blocked: 0,
                    cancelled,
                    overall_percent: 0,
                }
            };
        // 空项目：从未建过任务 → 不算收尾
        assert!(!mk(0, 0, 0, 0, 0).all_tasks_terminal());
        // 有活跃任务 → 不算
        assert!(!mk(3, 1, 1, 1, 0).all_tasks_terminal());
        assert!(!mk(2, 1, 0, 1, 0).all_tasks_terminal());
        // 全部终态（完成 + 取消混合）→ 算
        assert!(mk(3, 2, 0, 0, 1).all_tasks_terminal());
        assert!(mk(1, 1, 0, 0, 0).all_tasks_terminal());
    }
}
