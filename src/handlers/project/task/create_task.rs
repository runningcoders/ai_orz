//! Handler: POST /api/v1/tasks - Create a new task

use super::response;
use crate::models::events::TaskDispatchedEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateTaskRequest, CreateTaskResponse};
use common::enums::AssigneeType;
use common::error::{Result, bail_err};

/// Create a new task
#[register_handler_tool(
    id = "create_task",
    name = "Create Task",
    description = "Create a task with a required title and assignee_id, plus optional description, priority, tags, project_id, due date, and dependency task IDs; assignee_type defaults to Agent and root_user_id to the current user. dependencies holds the prerequisite task IDs forming the DAG: whenever this task depends on other tasks, their IDs MUST be listed, or the task will be treated as startable immediately and executed out of order. Returns the full task detail. When assigned to an agent, a task-assignment notification is sent to that agent automatically.",
    params = "common::api::CreateTaskRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_task(
    ctx: RequestContext,
    params: CreateTaskRequest,
) -> Result<CreateTaskResponse> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.title.trim().is_empty() {
        bail_err!(InvalidRequest, "task title 不能为空");
    }
    if params.assignee_id.trim().is_empty() {
        bail_err!(InvalidRequest, "assignee_id 不能为空");
    }

    // 归属用户是**身份**不是参数：任务落库的 `root_user_id` 会经派单消息一路传给
    // 被唤醒的 Agent，成为它的 `ctx.user_id`（工具凭据 / 用户画像的唯一来源）。
    // 允许调用方指定他人，等于开了一条「用 A 的凭据跑 B 的工具调用」的越权链。
    let root_user_id = resolve_root_user_id(&current_user_id, params.root_user_id.as_deref())?;

    let ctx = ctx
        .to_builder()
        .try_project_id(params.project_id.as_deref())
        .build();

    let assignee_type = params.assignee_type.unwrap_or(AssigneeType::Agent);
    let assignee_id = params.assignee_id.clone();
    let task_description = params.description.clone();

    let task = domain()
        .task_manage()
        .create_with_options(
            ctx.clone(),
            params.title.clone(),
            task_description.clone().unwrap_or_default(),
            params.priority.unwrap_or_default(),
            params.tags.unwrap_or_default(),
            root_user_id,
            assignee_type,
            assignee_id.clone(),
            params.project_id.clone(),
            params.due_at,
            params.dependencies.unwrap_or_default(),
            current_user_id.clone(),
            params.acceptance_criteria.clone().unwrap_or_default(),
        )
        .await?;

    // 【动作先行、消息后发】任务行已落库，这里只发布「派发」动作事件；
    // TaskAssignment 的消息类型、收件人、链锚点由 TaskDispatchConsumer 单点收口。
    // ⚠️ 该消费者是 Sync 内联执行：publish 返回时消息已落库，事件不进内存队列，
    // 因此不存在「任务已存在但重启后事件丢失」的静默死任务窗口。
    //
    // 身份三分解（不再做「用户身份中继」）：
    // - 派发者 = `initiator_agent_id`（本轮被唤醒的 Agent）；用户从管理面直接
    //   建任务时它为空，消费者退回以 `root_user_id`（用户本人）为派发者；
    // - 接收者 = `assignee_id`；
    // - 所属用户 = `root_user_id`，落消息行独立列，供工具凭据与用户画像使用。
    if assignee_type == AssigneeType::Agent {
        aop::publish(
            &ctx,
            TaskDispatchedEvent::new(
                task.id(),
                &task.po.title,
                task_description.as_deref().unwrap_or_default(),
                params.project_id.as_deref(),
                &assignee_id,
                &task.po.root_user_id,
                &task.po.initiator_agent_id,
                &task.po.trigger_message_id,
            ),
        )
        .await;
    }

    Ok(response::to_detail(&task))
}

/// 解析任务的归属用户：缺省取当前用户，显式指定时**必须**是当前用户本人
///
/// 为什么不允许指定他人：任务归属会随派单消息落 `MessagePo.root_user_id`，
/// 消费侧据它还原被唤醒 Agent 的 `ctx.user_id`（工具凭据、用户画像、`AI_ORZ_USER_ID`
/// 的唯一来源）。放开就等于让调用方借用别人的身份凭据。
fn resolve_root_user_id(current_user_id: &str, requested: Option<&str>) -> Result<String> {
    match requested.map(str::trim).filter(|s| !s.is_empty()) {
        Some(uid) if uid != current_user_id => bail_err!(
            InvalidRequest,
            "root_user_id 只能是当前用户（{}），不能指定他人",
            current_user_id
        ),
        Some(uid) => Ok(uid.to_string()),
        None => Ok(current_user_id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_current_user() {
        assert_eq!(
            resolve_root_user_id("user-1", None).expect("缺省归属当前用户"),
            "user-1"
        );
    }

    #[test]
    fn accepts_explicit_self() {
        // 前端会显式回传当前用户（历史习惯），不能因此被拦
        assert_eq!(
            resolve_root_user_id("user-1", Some("user-1")).expect("本人可显式指定"),
            "user-1"
        );
        // 空串视同缺省
        assert_eq!(
            resolve_root_user_id("user-1", Some("  ")).expect("空串按缺省处理"),
            "user-1"
        );
    }

    #[test]
    fn rejects_other_user() {
        let err =
            resolve_root_user_id("user-1", Some("user-2")).expect_err("不得把任务归属到他人名下");
        assert!(
            err.to_string().contains("不能指定他人"),
            "错误信息须点明不能指定他人，实际: {err}"
        );
    }
}
