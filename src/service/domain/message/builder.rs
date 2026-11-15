//! 消息内容构建器
//!
//! 为 Agent Loop Engine 的系统通知场景构建结构化消息内容：
//! - 任务完成交付回执 / 完成知会（动作伴生消息，见 `TaskCompletionConsumer`）
//! - 项目跟进通知（场景 3：定时补偿触发）
//! - 项目收尾检查通知（场景 3 变体）
//! - 任务调度指令（当前无生产调用点，保留给项目级编排复用）

use common::api::artifact::ArtifactDetail;
use common::enums::task::TaskStatus;
use common::models::ProjectProgressSummary;

/// 构建任务调度通知消息内容（**当前无生产调用点**）
///
/// ⚠️ 2026-11-15 起任务完成不再由 `TaskEventConsumer` 发这条调度指令：交付
/// 改由 `TaskCompletionConsumer`(Sync) 以带字段/结果/产物的回执直达归属用户。
/// 本函数保留作项目级编排文案的现成模板（项目完成事件阶段可能复用）。
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

/// 任务完成交付消息的拼装入参（2026-11-15 拍板：交付信息以任务维度为准）
///
/// ⚠️ 两类字段来源不同，不可混为一谈：
/// - **身份类**（`root_user_id` / `initiator_agent_id` / `assignee_id`）取
///   `TaskStatusChangedEvent` 的**发布时快照**，防止 Async 时间窗内被改过的行
///   把结果送给错的人。
/// - **内容类**（`execution_result` / `artifacts`）必须由消费者按 `task_id`
///   **现取**：`mark_done` 那一刻 Agent 可能还没写执行结果、还没落产物，随事件
///   快照会拿到空值，交付消息就退化成一条空洞通知。
pub struct TaskCompletionView<'a> {
    pub task_id: &'a str,
    pub task_title: &'a str,
    pub project_id: Option<&'a str>,
    pub assignee_id: &'a str,
    pub root_user_id: &'a str,
    pub initiator_agent_id: &'a str,
    pub status: TaskStatus,
    pub progress: i32,
    pub execution_result: Option<&'a str>,
    pub artifacts: &'a [ArtifactDetail],
}

/// `execution_result` 注入上限：任务详情是全文权威，消息只做高效通知
const COMPLETION_RESULT_LIMIT: usize = 4000;
const ARTIFACT_DESC_LIMIT: usize = 120;

/// 构建任务完成**交付消息**正文（直达归属用户）
///
/// 与 [`build_task_dispatch_content`] 的区别是本质性的：后者是给项目 Owner Agent
/// 的兜底调度指令（只有标题 / 状态 / 进度），本函数是给用户看的**交付载体** ——
/// 任务存在的意义是交付产物，所以正文必须带齐任务维度关键字段、执行结果摘要与
/// 关联产物清单；任何一类为空都要显式降级说明，不能静默留白。
///
/// 产物引用复用现成的 `[名称](artifact:<id>)` 协议，前端会渲染成可打开的 chip，
/// 不需要新增表字段或下载链接。
pub fn build_task_completion_content(v: &TaskCompletionView<'_>) -> String {
    let mut out = format!("## 任务完成回执：{}\n\n", v.task_title);

    out.push_str("| 字段 | 值 |\n| --- | --- |\n");
    out.push_str(&format!("| 任务 ID | `{}` |\n", v.task_id));
    out.push_str(&format!(
        "| 状态 | {}（进度 {}%） |\n",
        task_status_label(v.status),
        v.progress
    ));
    out.push_str(&format!(
        "| 所属项目 | {} |\n",
        v.project_id
            .filter(|s| !s.is_empty())
            .unwrap_or("无（游离任务）")
    ));
    out.push_str(&format!(
        "| 执行 Agent | {} |\n",
        if v.assignee_id.is_empty() {
            "未指派"
        } else {
            v.assignee_id
        }
    ));
    out.push_str(&format!(
        "| 任务发起者 | {} |\n",
        if v.initiator_agent_id.is_empty() {
            "无"
        } else {
            v.initiator_agent_id
        }
    ));
    out.push_str(&format!(
        "| 归属用户 | {} |\n",
        if v.root_user_id.is_empty() {
            "未指定"
        } else {
            v.root_user_id
        }
    ));

    // 执行结果：全文可能很长，任务详情页是全文权威，消息里带摘要 + 截断标记
    out.push_str("\n### 执行结果\n\n");
    match v.execution_result.map(str::trim).filter(|s| !s.is_empty()) {
        Some(result) => {
            out.push_str(&truncate_chars(result, COMPLETION_RESULT_LIMIT));
            out.push('\n');
        }
        None => out.push_str(
            "本任务未登记执行结果。完整进展可在任务详情页查看；若结果已通过其他消息送达，请以那条为准。\n",
        ),
    }

    // 交付产物：产物是任务交付的主要载体，逐条给可点开的引用
    out.push_str(&format!("\n### 交付产物（{} 项）\n\n", v.artifacts.len()));
    if v.artifacts.is_empty() {
        if v.project_id.is_some() {
            out.push_str("本任务未关联产物，交付内容以上方执行结果为准。\n");
        } else {
            // ⚠️ artifacts.project_id NOT NULL，游离任务不可能有产物 ——
            // 这不是异常，必须写明白，否则用户会以为交付丢了。
            out.push_str("无项目任务不落产物（产物硬绑定项目），交付内容以上方执行结果为准。\n");
        }
    } else {
        for a in v.artifacts {
            let desc = if a.description.trim().is_empty() {
                "—".to_string()
            } else {
                truncate_chars(a.description.trim(), ARTIFACT_DESC_LIMIT)
            };
            out.push_str(&format!("- [{}](artifact:{}) — {}\n", a.name, a.id, desc));
        }
        out.push_str("\n点击产物可在侧栏预览或下载原件。\n");
    }

    out
}

/// 给发起者 / 项目 owner 的完成知会（NotifyOnly，不承载结果复述）
///
/// ⚠️ 明确告诉收件方「交付已直达归属用户」：否则它会按老习惯再产一封 Final
/// 转发给用户，把刚收敛的唯一交付载体重新拆成两份。
pub fn build_task_completion_notice(
    task_title: &str,
    task_id: &str,
    artifact_count: usize,
) -> String {
    format!(
        "【任务完成知会】任务「{title}」（`{id}`）已完成。\
         交付回执（关键字段 / 执行结果 / {count} 项产物）已由框架**直接送达归属用户**，\
         无需再转发、无需再调用 send_message 通知用户。\
         请只在本轮内处理你职责范围内的编排事项（如检查后继任务、推进项目收尾）；\
         若没有需要推进的事项，直接结束本轮即可。",
        title = task_title,
        id = task_id,
        count = artifact_count,
    )
}

/// 待验收通知的拼装入参（2026-11-15：任务视角的「提交」动作伴生消息）
///
/// 与 [`TaskCompletionView`] 的区别：这是给**验收人**看的工作单，不是给用户的
/// 交付回执。因此正文必须把「事先约定的标准」和「事后交出的东西」并列摆出来，
/// 让 Owner 能逐条核对，而不是凭一句「我完成了」做判断。
pub struct TaskReviewView<'a> {
    pub task_id: &'a str,
    pub task_title: &'a str,
    pub project_id: Option<&'a str>,
    pub assignee_id: &'a str,
    pub root_user_id: &'a str,
    pub acceptance_criteria: &'a str,
    pub execution_result: Option<&'a str>,
    pub artifacts: &'a [ArtifactDetail],
}

/// 构建「请验收」通知正文（发给项目 Owner → 发起者 fallback）
pub fn build_task_pending_review_content(v: &TaskReviewView<'_>) -> String {
    let mut out = format!("## 待验收：{}\n\n", v.task_title);

    out.push_str("| 字段 | 值 |\n| --- | --- |\n");
    out.push_str(&format!("| 任务 ID | `{}` |\n", v.task_id));
    out.push_str(&format!(
        "| 所属项目 | {} |\n",
        v.project_id
            .filter(|s| !s.is_empty())
            .unwrap_or("无（游离任务）")
    ));
    out.push_str(&format!(
        "| 执行 Agent | {} |\n",
        if v.assignee_id.is_empty() {
            "未指派"
        } else {
            v.assignee_id
        }
    ));
    out.push_str(&format!(
        "| 归属用户 | {} |\n",
        if v.root_user_id.is_empty() {
            "未指定"
        } else {
            v.root_user_id
        }
    ));
    out.push_str("| 当前状态 | 待验收（未计入完成，后继任务未解锁） |\n");

    out.push_str("\n### 验收标准（派单时约定）\n\n");
    let criteria = v.acceptance_criteria.trim();
    if criteria.is_empty() {
        out.push_str(
            "本任务未登记验收标准。请按任务描述、执行结果与下方产物自行判断是否达标；建议后续补标准。\n",
        );
    } else {
        out.push_str(&format!("{criteria}\n"));
    }

    out.push_str("\n### 执行人提交说明\n\n");
    match v.execution_result.map(str::trim).filter(|s| !s.is_empty()) {
        Some(result) => {
            out.push_str(&truncate_chars(result, COMPLETION_RESULT_LIMIT));
            out.push('\n');
        }
        None => out.push_str(
            "执行人未填写提交说明。请用 get_task(with_artifacts=true) 核对任务详情与产物后再决定。\n",
        ),
    }

    out.push_str(&format!("\n### 交付产物（{} 项）\n\n", v.artifacts.len()));
    if v.artifacts.is_empty() {
        out.push_str("本任务未关联产物，交付内容以上方提交说明为准。\n");
    } else {
        for a in v.artifacts {
            let desc = if a.description.trim().is_empty() {
                "—".to_string()
            } else {
                truncate_chars(a.description.trim(), ARTIFACT_DESC_LIMIT)
            };
            out.push_str(&format!("- [{}](artifact:{}) — {}\n", a.name, a.id, desc));
        }
        out.push_str("\n点击产物可在侧栏预览或下载原件。\n");
    }

    out.push_str(&format!(
        "\n### 请执行验收\n\n\
         - 达标：调用 `confirm_task(task_id=\"{}\", note=\"验收结论\")`。\n\
         - 不达标：调用 `reject_task(task_id=\"{}\", reason=\"具体待补事项\")`，任务会打回进行中并通知执行人返工。\n\n\
         ⚠️ 确认通过后，框架会**直接**把带字段/结果/产物的交付回执送达归属用户并解锁 DAG 后继；\
         无需自行转发，也不要调用 send_message 通知用户。",
        v.task_id, v.task_id,
    ));

    out
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n\n…（内容过长已截断，完整内容见任务详情页）");
    out
}

/// 构建项目跟进通知消息内容（场景 3：定时补偿触发）
///
/// `progress` 为消费侧预取的进度汇总（省去 Agent 的一跳查询，也让通知更具体）；
/// 传 `None` 时不注入快照块（保持正文与历史一致）。
///
/// 触发器以**系统**身份发出本通知（`from_role=System`），路由规则
/// `owner_briefing` 把 Agent 的 Final 交给项目归属用户——因此正文把「通知用户」
/// 收敛为「最终回复的写法要求」，不再要求调 send_message（那会造成双重通知）。
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
        TaskStatus::PendingReview => "待验收",
    }
}
#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
