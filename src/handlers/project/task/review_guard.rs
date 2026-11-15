//! 验收动作的调用方身份守卫（confirm_task / reject_task 共用）
//!
//! 交付责任制里「谁能验收」是任务侧契约，不能靠工具包授权代替：
//! `project_management` 标签只说明这个 Agent 会用项目工具，不说明它是
//! 这个任务的 Owner 或发起者。管理面 / 系统路径（无 `agent_id`）不受限。

use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use common::error::Result;

/// 校验调用方是否为该任务的验收责任人
///
/// 允许集合 = 任务发起者 ∪ 项目 Owner。**集合为空即拒绝**：既有发起者又有
/// Owner 都解析不出来，说明这张工单没有任何可核对的验收主体，Agent 以任何
/// 身份确认都是「自己给自己盖章」。这类任务（用户直建、项目行缺失）的收敛
/// 路径是管理面确认，或者由 `TaskPendingReviewConsumer` 自动确认——都不经过
/// 本守卫。
///
/// ⚠️ **fail-closed**：项目查询失败（DB 故障 / 超时）必须报错，不能与「项目
/// 不存在」一起被吞成「没有 Owner」。后者会让 `allowed` 退化成只剩发起者，
/// 发起者也空时就成了任意 Agent 都能验收任意任务。「项目不存在」（`Ok(None)`）
/// 是数据不一致而非故障，只记日志继续 —— 但继续的前提是发起者仍在集合里。
pub(super) async fn ensure_reviewer(ctx: &RequestContext, task: &Task) -> Result<()> {
    let Some(caller) = ctx.agent_id() else {
        // 无 Agent 身份 = HTTP 管理面 / 系统触发，走既有角色权限体系。
        return Ok(());
    };

    let mut allowed: Vec<String> = Vec::new();
    if !task.po.initiator_agent_id.is_empty() {
        allowed.push(task.po.initiator_agent_id.clone());
    }
    if let Some(project_id) = task.po.project_id.as_deref() {
        match project::domain()
            .project_manage()
            .get(ctx.clone(), project_id)
            .await
        {
            Ok(Some(p)) => {
                if let Some(owner) = p.po.owner_agent_id.as_deref()
                    && !owner.is_empty()
                {
                    allowed.push(owner.to_string());
                }
            }
            Ok(None) => {
                // 任务挂着不存在的项目：数据不一致，不是查询故障。按「无项目」
                // 处理（只剩发起者），同时留痕便于排查脏数据。
                log_warn!(
                    ctx,
                    "task_review_guard",
                    task_id = %task.po.id,
                    project_id = %project_id,
                    "任务归属的项目不存在，验收责任人只按任务发起者判定"
                );
            }
            Err(e) => {
                log_warn!(
                    ctx,
                    "task_review_guard",
                    task_id = %task.po.id,
                    project_id = %project_id,
                    error = ?e,
                    "查询项目失败，无法确认验收责任人"
                );
                // 不放行：查不到 Owner 时放行等于给任意 Agent 开验收后门。
                return Err(common::error::err!(
                    Internal,
                    "无法确认任务 {} 的验收责任人（项目 {} 查询失败），请稍后重试",
                    task.po.id,
                    project_id
                ));
            }
        }
    }

    if allowed.is_empty() {
        log_warn!(
            ctx,
            "task_review_guard",
            task_id = %task.po.id,
            caller = %caller,
            "任务既无发起者也无项目 Owner，Agent 验收被拒绝（应由管理面或自动确认收敛）"
        );
        return Err(common::error::err!(
            InvalidRequest,
            "任务 {} 没有可核对的验收责任人（既无任务发起者，也无项目 Owner），Agent {} 不能验收；请由项目负责人或管理面确认",
            task.po.id,
            caller
        ));
    }
    if allowed.iter().any(|a| a == caller.as_str()) {
        return Ok(());
    }

    Err(common::error::err!(
        InvalidRequest,
        "Agent {} 不是任务 {} 的发起者或项目 Owner，无权验收（允许：{:?}）",
        caller,
        task.po.id,
        allowed
    ))
}
