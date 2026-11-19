//! Handler: PUT /api/v1/projects/{project_id}/owner — 交接项目负责人
//!
//! 负责人不是普通字段，而是**权限**：它同时是「谁能验收本项目任务」
//!（`task_acceptance::review_guard::ensure_reviewer`）与「任务完成知会先发给谁」
//!（`task_completion_consumer`）的判据。所以交接独立成一个接口 + 一个工具，
//! 而不是 `update_project` 的一个可选字段 —— 否则任何能改项目描述的 Agent 都能
//! 顺手把验收权转给自己。

use super::response;
use crate::models::project::Project;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{TransferProjectOwnerRequest, TransferProjectOwnerResponse};
use common::error::Result;

/// 交接项目负责人
#[register_handler_tool(
    id = "transfer_project_owner",
    name = "Transfer Project Owner",
    description = "Hand a project over to another agent, or clear the owner by passing an empty owner_agent_id. The owner is the acceptance reviewer for every task in the project and the first recipient of task-completion notices, so this is a permission transfer rather than a field edit. Only the current owner (or any caller for an owner-less project) may transfer it; passing the current owner is a no-op. Returns the updated project detail.",
    params = "common::api::TransferProjectOwnerRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn transfer_project_owner(
    ctx: RequestContext,
    params: TransferProjectOwnerRequest,
) -> Result<TransferProjectOwnerResponse> {
    let project = domain()
        .project_manage()
        .get(ctx.clone(), &params.project_id)
        .await?
        .ok_or_else(|| common::error::err!(NotFound, "Project {} not found", params.project_id))?;

    ensure_can_transfer(&ctx, &project)?;
    ensure_target_agent_exists(&ctx, &project, &params.owner_agent_id).await?;

    let modified_by = ctx.uid();
    let project = domain()
        .project_manage()
        .transfer_owner(
            ctx,
            &params.project_id,
            params.owner_agent_id,
            params.note,
            modified_by,
        )
        .await?;

    Ok(response::to_detail(&project))
}

/// 谁能交接负责人
///
/// - 管理面 / 系统触发（无 `agent_id`）→ 放行：这是用户给自己项目换负责人的正常路径；
/// - Agent → 必须是**当前负责人**。无主项目（`owner_agent_id` 为空）放行，与
///   `review_guard` 的「无责任人」口径一致：否则项目一旦没有负责人，就再也没人
///   能把负责人补回来。
///
/// ⚠️ 这里只判「能不能交出去」，不判「交付给谁」——目标合法性（Agent 是否存在）
/// 由 [`ensure_target_agent_exists`] 单独校验，两道守卫各管一件事。
fn ensure_can_transfer(ctx: &RequestContext, project: &Project) -> Result<()> {
    let Some(caller) = ctx.agent_id() else {
        return Ok(());
    };
    match project.po.owner_agent_id.as_deref() {
        Some(owner) if !owner.is_empty() && owner != caller.as_str() => Err(common::error::err!(
            InvalidRequest,
            "Agent {} 不是项目 {} 的负责人（当前负责人：{}），无权交接",
            caller,
            project.po.id,
            owner
        )),
        _ => Ok(()),
    }
}

/// 目标 Agent 必须存在（空串 = 清空负责人，不校验）
///
/// 放在 handler 而不是 domain：这是**参数合法性**，跨域查 hr 属于编排；
/// 由 handler 先调 hr 再调 project，两个域之间没有调用。
/// 与 domain 的幂等短路同口径：目标 == 现任时不校验（现任被删也不挡交接）。
async fn ensure_target_agent_exists(
    ctx: &RequestContext,
    project: &Project,
    owner_agent_id: &str,
) -> Result<()> {
    let target = owner_agent_id.trim();
    let current = project.po.owner_agent_id.as_deref().unwrap_or_default();
    if target.is_empty() || target == current {
        return Ok(());
    }
    let agent = crate::service::domain::hr::domain()
        .agent_manage()
        .get_agent(
            ctx.clone(),
            target,
            crate::service::dal::agent::AgentFetchOptions::default(),
        )
        .await?;
    if agent.is_none() {
        return Err(common::error::err!(
            NotFound,
            "接手的 Agent 不存在: {}",
            target
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;

    /// 测试用 ctx（懒连接内存池，不触库）。`agent_id=None` 即管理面调用方。
    fn test_ctx(agent_id: Option<&str>) -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("内存池");
        let ctx = new_test_ctx("user-1", pool);
        match agent_id {
            Some(aid) => ctx.to_builder().agent_id(aid).build(),
            None => ctx,
        }
    }

    fn project_with_owner(owner: Option<&str>) -> Project {
        Project::new(
            "p-1".to_string(),
            "Project".to_string(),
            String::new(),
            None,
            None,
            0,
            Vec::new(),
            "user-1".to_string(),
            owner.map(|s| s.to_string()),
            None,
            None,
            None,
            "user-1".to_string(),
        )
    }

    #[tokio::test]
    async fn management_plane_may_always_transfer() {
        // 无 agent_id = 管理面用户，是「给自己的项目换负责人」的正常路径
        let ctx = test_ctx(None);
        assert!(ensure_can_transfer(&ctx, &project_with_owner(Some("agent-a"))).is_ok());
    }

    #[tokio::test]
    async fn current_owner_may_transfer() {
        let ctx = test_ctx(Some("agent-a"));
        assert!(ensure_can_transfer(&ctx, &project_with_owner(Some("agent-a"))).is_ok());
    }

    #[tokio::test]
    async fn non_owner_agent_is_rejected() {
        let ctx = test_ctx(Some("agent-b"));
        let err = ensure_can_transfer(&ctx, &project_with_owner(Some("agent-a")))
            .expect_err("非负责人不得交接");
        assert!(
            err.to_string().contains("无权交接"),
            "错误信息须说明无权交接，实际: {err}"
        );
    }

    #[tokio::test]
    async fn ownerless_project_can_be_claimed() {
        // 无主项目必须留一个认领入口，否则负责人一旦被清空就再也补不回来
        let ctx = test_ctx(Some("agent-b"));
        assert!(ensure_can_transfer(&ctx, &project_with_owner(None)).is_ok());
    }
}
