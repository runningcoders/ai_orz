//! tests 单元测试（拆分自 builder.rs）
//!
//! 文件瘦身：原 516 行 → 297 行，测试体 220 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
    // 巡检由系统触发、结论经 owner_briefing 直达归属用户 → 无需（也不该）再 send_message
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
    // 巡检由系统触发、结论经 owner_briefing 直达归属用户 → 无需再 send_message
    assert!(content.contains("自动送达"));
    assert!(content.contains("无需"));
}

/// `all_tasks_terminal` 判定边界：有活跃任务/空项目不算，终态组合（含取消）算
#[test]
fn test_progress_summary_all_tasks_terminal() {
    let mk = |total: usize, completed: usize, in_prog: usize, pending: usize, cancelled: usize| {
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

fn artifact_fixture() -> ArtifactDetail {
    serde_json::from_value(serde_json::json!({
        "id": "art_report",
        "project_id": "p1",
        "task_id": "t1",
        "name": "验收报告",
        "description": "含测试结论",
        "file_type": "Document",
        "source_type": "generated_content",
        "file_path": "report.md",
        "mime_type": "text/markdown",
        "file_size": 128,
        "tags": [],
        "status": 1,
        "created_by": "u1",
        "modified_by": "u1",
        "created_at": 0,
        "updated_at": 0
    }))
    .expect("artifact fixture should deserialize")
}

/// 交付回执必须自包含：任务维度关键字段 + 执行结果 + 可点开的产物清单
#[test]
fn completion_delivery_carries_fields_result_and_artifacts() {
    let artifacts = vec![artifact_fixture()];
    let content = build_task_completion_content(&TaskCompletionView {
        task_id: "task-42",
        task_title: "搭建脚手架",
        project_id: Some("p1"),
        assignee_id: "agent-worker",
        root_user_id: "user-1",
        initiator_agent_id: "agent-reception",
        status: TaskStatus::Completed,
        progress: 100,
        execution_result: Some("## 完成情况\n已交付脚手架与 CI 配置"),
        artifacts: &artifacts,
    });
    for expected in [
        "task-42",
        "搭建脚手架",
        "agent-worker",
        "agent-reception",
        "user-1",
        "p1",
        "已交付脚手架与 CI 配置",
        "[验收报告](artifact:art_report)",
        "含测试结论",
    ] {
        assert!(
            content.contains(expected),
            "交付回执缺少关键字段/产物 {expected:?}：{content}"
        );
    }
}

/// 无项目任务没有产物通道（artifacts.project_id NOT NULL），空值必须显式降级，
/// 不能静默留白变成一条空洞通知 —— 这是本设计的底线。
#[test]
fn completion_delivery_degrades_explicitly_without_artifacts() {
    let content = build_task_completion_content(&TaskCompletionView {
        task_id: "free-task",
        task_title: "回答一个咨询",
        project_id: None,
        assignee_id: "agent-worker",
        root_user_id: "user-1",
        initiator_agent_id: "",
        status: TaskStatus::Completed,
        progress: 100,
        execution_result: None,
        artifacts: &[],
    });
    assert!(content.contains("未登记执行结果"), "{content}");
    assert!(content.contains("无项目任务不落产物"), "{content}");
    assert!(content.contains("无（游离任务）"), "{content}");
    assert!(content.contains("无"), "{content}");
}

/// 长 execution_result 被截断且标明全文位置，避免把消息撑爆又不说明来源
#[test]
fn completion_delivery_truncates_long_result() {
    let long = "结".repeat(COMPLETION_RESULT_LIMIT + 100);
    let content = build_task_completion_content(&TaskCompletionView {
        task_id: "t",
        task_title: "长结果",
        project_id: Some("p"),
        assignee_id: "a",
        root_user_id: "u",
        initiator_agent_id: "",
        status: TaskStatus::Completed,
        progress: 100,
        execution_result: Some(&long),
        artifacts: &[],
    });
    assert!(content.contains("内容过长已截断"), "{content}");
    assert!(content.chars().count() < COMPLETION_RESULT_LIMIT + 400);
}

/// 知会必须写明「交付已直达用户」，否则收件方会再转发一份、把唯一载体拆成两份
#[test]
fn completion_notice_forbids_second_delivery() {
    let notice = build_task_completion_notice("搭建脚手架", "task-42", 2);
    assert!(notice.contains("直接送达归属用户"), "{notice}");
    assert!(notice.contains("无需再转发"), "{notice}");
    assert!(notice.contains("2 项产物"), "{notice}");
}

fn review_view<'a>(
    criteria: &'a str,
    result: Option<&'a str>,
    artifacts: &'a [ArtifactDetail],
) -> TaskReviewView<'a> {
    TaskReviewView {
        task_id: "task-42",
        task_title: "搭建脚手架",
        project_id: Some("p1"),
        assignee_id: "agent-worker",
        root_user_id: "user-1",
        acceptance_criteria: criteria,
        execution_result: result,
        artifacts,
    }
}

/// 待验收工作单必须让 Owner 能「对照标准核对交付」，且明确给出两种验收动作：
/// 少任何一块，Owner 只能凭一句「我做完了」猜，验收层就退化成摆设。
#[test]
fn pending_review_notice_carries_criteria_result_artifacts_and_verdict_tools() {
    let artifacts = vec![artifact_fixture()];
    let content = build_task_pending_review_content(&review_view(
        "1. CI 流水线可运行\n2. 附部署说明",
        Some("已交付脚手架与 CI 配置"),
        &artifacts,
    ));
    for expected in [
        "task-42",
        "搭建脚手架",
        "1. CI 流水线可运行",
        "已交付脚手架与 CI 配置",
        "[验收报告](artifact:art_report)",
        "confirm_task",
        "reject_task",
        "未计入完成，后继任务未解锁",
        "无需自行转发",
    ] {
        assert!(
            content.contains(expected),
            "待验收工作单缺少 {expected:?}：{content}"
        );
    }
    // 待验收不是交付回执：不许出现「任务完成回执」这种会让用户误以为已收口的标题
    assert!(!content.contains("任务完成回执"), "{content}");
}

/// 没登记验收标准 / 没写提交说明 / 没有产物时，三类空值都必须显式降级说明，
/// 不能静默留白 —— 与完成交付回执同一条底线。
#[test]
fn pending_review_notice_degrades_explicitly_when_materials_are_missing() {
    let content = build_task_pending_review_content(&review_view("  ", None, &[]));
    assert!(content.contains("未登记验收标准"), "{content}");
    assert!(content.contains("未填写提交说明"), "{content}");
    assert!(content.contains("本任务未关联产物"), "{content}");
    // 降级了也不能把验收动作指引丢掉
    assert!(content.contains("confirm_task"), "{content}");
}

/// 状态措辞是硬约束：待验收 ≠ 已完成，正文必须写清 DAG 与交付都还没触发，
/// 否则 Owner 会以为「执行人 mark_done 就已经交付了」而跳过 confirm_task。
#[test]
fn pending_review_notice_never_claims_completion() {
    let content = build_task_pending_review_content(&review_view("可运行", Some("做完了"), &[]));
    assert!(content.contains("待验收"), "{content}");
    assert!(content.contains("确认通过后"), "{content}");
    assert!(
        content.contains("会**直接**把带字段/结果/产物的交付回执送达归属用户并解锁 DAG 后继"),
        "{content}"
    );
}
