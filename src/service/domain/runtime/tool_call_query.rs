//! Runtime Tool Call 查询辅助逻辑

use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallStatus;
use crate::pkg::tool_tracing::logger::{MAX_TOOL_CALL_QUERY_LIMIT, ToolCallQuery};
use common::enums::{CallerType, UserRole};
use common::error::Result;

/// Merge explicit query scope with RequestContext scope and fail closed when no scope exists.
///
/// 按调用方类型分流（作用域模型）：
/// - **Agent / System 调用**：ctx 自带 agent/project/task 作用域，查询条件必须与之一致，
///   fail-closed 防止 Agent 侧越权读取他人 trace（原逻辑不变）
/// - **User 调用（Web 端 JWT 请求）**：ctx 天然无作用域（请求头不注入 agent/project/task），
///   作用域只能由查询条件显式指定：
///   - Admin / SuperAdmin：可全量查询（可观测性管理页）
///   - Member：必须至少提供 call_id / agent_id / project_id / task_id 之一，禁止无边界遍历
pub(crate) fn with_context_scope(
    ctx: RequestContext,
    mut query: ToolCallQuery,
) -> Result<ToolCallQuery> {
    if ctx.caller_type == CallerType::User {
        // 角色缺失时按普通成员处理（UserRole::default() 是 SuperAdmin，绝不能当默认值兜底）
        let is_admin = ctx
            .user_role
            .map(UserRole::from)
            .is_some_and(|role| matches!(role, UserRole::Admin | UserRole::SuperAdmin));
        let has_explicit_scope = query.call_id.is_some()
            || query.agent_id.is_some()
            || query.project_id.is_some()
            || query.task_id.is_some();
        if !is_admin && !has_explicit_scope {
            return Err(common::error::Error::bad_request(
                "tool call query requires at least one of call_id / agent_id / project_id / task_id"
                    .to_string(),
            ));
        }
        // 用户调用：ctx 无作用域，跳过错配校验，直接使用查询条件（limit 校验仍生效）
        if let Some(limit) = query.limit
            && limit > MAX_TOOL_CALL_QUERY_LIMIT
        {
            return Err(common::error::Error::bad_request(format!(
                "tool call query limit must be <= {MAX_TOOL_CALL_QUERY_LIMIT}"
            )));
        }
        return Ok(query);
    }

    if ctx.agent_id.is_none() && ctx.project_id.is_none() && ctx.task_id.is_none() {
        return Err(common::error::Error::bad_request(
            "tool call query requires scoped request context".to_string(),
        ));
    }

    ensure_scope_does_not_conflict(
        "agent_id",
        ctx.agent_id.as_deref(),
        query.agent_id.as_deref(),
    )?;
    ensure_scope_does_not_conflict(
        "project_id",
        ctx.project_id.as_deref(),
        query.project_id.as_deref(),
    )?;
    ensure_scope_does_not_conflict("task_id", ctx.task_id.as_deref(), query.task_id.as_deref())?;

    if let Some(limit) = query.limit
        && limit > MAX_TOOL_CALL_QUERY_LIMIT
    {
        return Err(common::error::Error::bad_request(format!(
            "tool call query limit must be <= {MAX_TOOL_CALL_QUERY_LIMIT}"
        )));
    }

    if query.agent_id.is_none() {
        query.agent_id = ctx.agent_id.clone();
    }
    if query.project_id.is_none() {
        query.project_id = ctx.project_id.clone();
    }
    if query.task_id.is_none() {
        query.task_id = ctx.task_id.clone();
    }

    Ok(query)
}

pub(crate) fn ensure_call_id_present(query: &ToolCallQuery) -> Result<()> {
    match query.call_id.as_deref() {
        Some(call_id) if !call_id.trim().is_empty() => Ok(()),
        _ => Err(common::error::Error::bad_request(
            "tool call detail lookup requires call_id".to_string(),
        )),
    }
}

fn ensure_scope_does_not_conflict(
    field: &str,
    context_value: Option<&str>,
    query_value: Option<&str>,
) -> Result<()> {
    match (context_value, query_value) {
        (Some(context_value), Some(query_value)) if context_value != query_value => {
            Err(common::error::Error::bad_request(format!(
                "tool call query {field} conflicts with request context"
            )))
        }
        (None, Some(_)) => Err(common::error::Error::bad_request(format!(
            "tool call query {field} requires matching request context scope"
        ))),
        _ => Ok(()),
    }
}

pub(crate) fn status_from_dto(status: common::api::ToolCallStatusDto) -> ToolCallStatus {
    match status {
        common::api::ToolCallStatusDto::Started => ToolCallStatus::Started,
        common::api::ToolCallStatusDto::Completed => ToolCallStatus::Completed,
        common::api::ToolCallStatusDto::Failed => ToolCallStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试 ctx（惰性内存池，不建立真实连接——本组单测只校验作用域规则，不查库）
    fn ctx_with(caller: CallerType, role: Option<i32>, scope: Option<&str>) -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy pool");
        let mut builder = crate::pkg::request_context_test_support::new_test_ctx("user-1", pool)
            .to_builder()
            .caller_type(caller)
            .organization_id("org-1");
        if let Some(role) = role {
            builder = builder.user_role(role);
        }
        if let Some(agent_id) = scope {
            builder = builder.agent_id(agent_id);
        }
        builder.build()
    }

    fn empty_query() -> ToolCallQuery {
        ToolCallQuery {
            call_id: None,
            agent_id: None,
            project_id: None,
            task_id: None,
            tool_id: None,
            status: None,
            started_after: None,
            started_before: None,
            limit: None,
        }
    }

    #[tokio::test]
    async fn user_member_requires_explicit_scope() {
        let ctx = ctx_with(CallerType::User, Some(UserRole::Member as i32), None);
        // 无任何过滤 → 拒绝（禁止无边界遍历 trace）
        assert!(with_context_scope(ctx.clone(), empty_query()).is_err());
        // 带 explicit 作用域 → 通过
        let mut query = empty_query();
        query.agent_id = Some("agent-1".to_string());
        let merged = with_context_scope(ctx, query).expect("member with agent scope");
        assert_eq!(merged.agent_id.as_deref(), Some("agent-1"));
    }

    #[tokio::test]
    async fn user_member_call_id_only_is_allowed() {
        let ctx = ctx_with(CallerType::User, Some(UserRole::Member as i32), None);
        let mut query = empty_query();
        query.call_id = Some("call-1".to_string());
        assert!(with_context_scope(ctx, query).is_ok());
    }

    #[tokio::test]
    async fn user_admin_may_query_unscoped() {
        for role in [UserRole::Admin, UserRole::SuperAdmin] {
            let ctx = ctx_with(CallerType::User, Some(role as i32), None);
            assert!(
                with_context_scope(ctx, empty_query()).is_ok(),
                "role={role:?}"
            );
        }
    }

    #[tokio::test]
    async fn agent_scope_stays_fail_closed() {
        // ctx 无作用域 → 拒绝
        let ctx = ctx_with(CallerType::Agent, None, None);
        assert!(with_context_scope(ctx, empty_query()).is_err());

        // 查询作用域与 ctx 不一致 → 拒绝（防越权）
        let ctx = ctx_with(CallerType::Agent, None, Some("agent-1"));
        let mut query = empty_query();
        query.agent_id = Some("agent-2".to_string());
        assert!(with_context_scope(ctx, query).is_err());

        // 查询作用域与 ctx 一致 → 通过
        let ctx = ctx_with(CallerType::Agent, None, Some("agent-1"));
        let mut query = empty_query();
        query.agent_id = Some("agent-1".to_string());
        assert!(with_context_scope(ctx, query).is_ok());
    }

    #[tokio::test]
    async fn user_query_limit_is_validated() {
        let ctx = ctx_with(CallerType::User, Some(UserRole::Admin as i32), None);
        let mut query = empty_query();
        query.limit = Some(MAX_TOOL_CALL_QUERY_LIMIT + 1);
        assert!(with_context_scope(ctx, query).is_err());
    }
}
