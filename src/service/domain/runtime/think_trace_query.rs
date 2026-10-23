//! Runtime Think Trace 查询作用域校验
//!
//! 轨迹文件位于 `agents/{agent_id}/think_trace/`，本身不带鉴权信息，因此
//! 必须在 Domain 层统一收窄查询范围（REST 与神经工具共用同一实现）：
//! - **Agent / System 调用**：请求上下文自带 agent/project/task 作用域时，
//!   查询条件必须与之一致；缺省字段用上下文补齐（fail-closed）。
//! - **User 调用（Web 端 JWT）**：ctx 天然无作用域；Admin/SuperAdmin 可全量
//!   查询，普通成员必须至少显式提供 agent_id / project_id / task_id 之一。
//!
//! 校验口径与 [`super::tool_call_query`] 保持一致，避免两个可观测性入口出现
//! 「工具轨迹能查、思考轨迹能越权」的权限缝隙。
//!
//! [`with_context_fields`] 是两种轨迹（think / memory）共用的校验内核：
//! memory trace 没有 project 维度（`MemoryTrace` 不带 project_id），调用方传
//! `allow_project = false`，否则「只有 project 作用域」的请求会退化成全量查询。

use crate::pkg::request_context::RequestContext;
use crate::pkg::think_trace::{MAX_THINK_TRACE_QUERY_LIMIT, ThinkTraceQuery};
use common::enums::{CallerType, UserRole};
use common::error::Result;

/// 校验并合并后的作用域三元组
#[derive(Debug, Clone, Default)]
pub(crate) struct TraceScope {
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

/// 共享校验内核入参
///
/// 字段较多，聚成结构体避免 `too_many_arguments`，也让两种轨迹的调用点
/// 显式标注各自支持的维度。
#[derive(Debug, Clone)]
pub(crate) struct TraceScopeFields<'a> {
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub limit: Option<usize>,
    pub max_limit: usize,
    /// 该轨迹类型是否支持按 project 维度收窄；不支持时 project 不参与
    /// 「是否具备作用域」与冲突校验，避免退化为无过滤全量查询
    pub allow_project: bool,
    /// 错误信息中的轨迹名称（`think trace` / `memory trace`）
    pub noun: &'a str,
}

pub(crate) fn with_context_fields(
    ctx: RequestContext,
    fields: TraceScopeFields<'_>,
) -> Result<TraceScope> {
    let TraceScopeFields {
        agent_id,
        project_id,
        task_id,
        limit,
        max_limit,
        allow_project,
        noun,
    } = fields;
    ensure_limit(limit, max_limit, noun)?;

    if ctx.caller_type == CallerType::User {
        // 角色缺失时按普通成员处理（UserRole::default() 是 SuperAdmin，绝不能兜底）
        let is_admin = ctx
            .user_role
            .map(UserRole::from)
            .is_some_and(|role| matches!(role, UserRole::Admin | UserRole::SuperAdmin));
        let has_explicit_scope =
            agent_id.is_some() || (allow_project && project_id.is_some()) || task_id.is_some();
        if !is_admin && !has_explicit_scope {
            return Err(common::error::Error::bad_request(format!(
                "{noun} query requires at least one of agent_id / project_id / task_id"
            )));
        }
        return Ok(TraceScope {
            agent_id,
            project_id,
            task_id,
        });
    }

    // Agent / System：必须由上下文提供至少一个作用域维度
    let has_context_scope = ctx.agent_id.is_some()
        || (allow_project && ctx.project_id.is_some())
        || ctx.task_id.is_some();
    if !has_context_scope {
        return Err(common::error::Error::bad_request(format!(
            "{noun} query requires scoped request context"
        )));
    }

    ensure_scope_does_not_conflict(
        "agent_id",
        ctx.agent_id.as_deref(),
        agent_id.as_deref(),
        noun,
    )?;
    if allow_project {
        ensure_scope_does_not_conflict(
            "project_id",
            ctx.project_id.as_deref(),
            project_id.as_deref(),
            noun,
        )?;
    }
    ensure_scope_does_not_conflict("task_id", ctx.task_id.as_deref(), task_id.as_deref(), noun)?;

    Ok(TraceScope {
        agent_id: agent_id.or_else(|| ctx.agent_id.clone()),
        project_id: if allow_project {
            project_id.or_else(|| ctx.project_id.clone())
        } else {
            project_id
        },
        task_id: task_id.or_else(|| ctx.task_id.clone()),
    })
}

/// 合并显式查询作用域与请求上下文作用域
pub(crate) fn with_context_scope(
    ctx: RequestContext,
    mut query: ThinkTraceQuery,
) -> Result<ThinkTraceQuery> {
    let scope = with_context_fields(
        ctx,
        TraceScopeFields {
            agent_id: query.agent_id.clone(),
            project_id: query.project_id.clone(),
            task_id: query.task_id.clone(),
            limit: query.pagination.limit,
            max_limit: MAX_THINK_TRACE_QUERY_LIMIT,
            allow_project: true,
            noun: "think trace",
        },
    )?;
    query.agent_id = scope.agent_id;
    query.project_id = scope.project_id;
    query.task_id = scope.task_id;
    Ok(query)
}

fn ensure_limit(limit: Option<usize>, max_limit: usize, noun: &str) -> Result<()> {
    if let Some(limit) = limit
        && limit > max_limit
    {
        return Err(common::error::Error::bad_request(format!(
            "{noun} query limit must be <= {max_limit}"
        )));
    }
    Ok(())
}

fn ensure_scope_does_not_conflict(
    field: &str,
    context_value: Option<&str>,
    query_value: Option<&str>,
    noun: &str,
) -> Result<()> {
    match (context_value, query_value) {
        (Some(context_value), Some(query_value)) if context_value != query_value => {
            Err(common::error::Error::bad_request(format!(
                "{noun} query {field} conflicts with request context"
            )))
        }
        (None, Some(_)) => Err(common::error::Error::bad_request(format!(
            "{noun} query {field} requires matching request context scope"
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn user_member_requires_explicit_scope() {
        let ctx = ctx_with(CallerType::User, Some(UserRole::Member as i32), None);
        assert!(with_context_scope(ctx.clone(), ThinkTraceQuery::default()).is_err());

        let query = ThinkTraceQuery {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        };
        let merged = with_context_scope(ctx, query).expect("member with agent scope");
        assert_eq!(merged.agent_id.as_deref(), Some("agent-1"));
    }

    #[tokio::test]
    async fn user_admin_may_query_unscoped() {
        for role in [UserRole::Admin, UserRole::SuperAdmin] {
            let ctx = ctx_with(CallerType::User, Some(role as i32), None);
            assert!(
                with_context_scope(ctx, ThinkTraceQuery::default()).is_ok(),
                "role={role:?}"
            );
        }
    }

    #[tokio::test]
    async fn agent_scope_stays_fail_closed() {
        let ctx = ctx_with(CallerType::Agent, None, None);
        assert!(with_context_scope(ctx, ThinkTraceQuery::default()).is_err());

        let ctx = ctx_with(CallerType::Agent, None, Some("agent-1"));
        let query = ThinkTraceQuery {
            agent_id: Some("agent-2".to_string()),
            ..Default::default()
        };
        assert!(with_context_scope(ctx, query).is_err());

        let ctx = ctx_with(CallerType::Agent, None, Some("agent-1"));
        let merged = with_context_scope(ctx, ThinkTraceQuery::default()).expect("own scope");
        assert_eq!(merged.agent_id.as_deref(), Some("agent-1"));
    }

    #[tokio::test]
    async fn query_limit_is_validated() {
        let ctx = ctx_with(CallerType::User, Some(UserRole::Admin as i32), None);
        let query = ThinkTraceQuery {
            pagination: common::api::PaginationParams {
                limit: Some(MAX_THINK_TRACE_QUERY_LIMIT + 1),
                offset: None,
            },
            ..Default::default()
        };
        assert!(with_context_scope(ctx, query).is_err());
    }

    #[tokio::test]
    async fn project_scope_is_ignored_when_unsupported() {
        // 只有 project 作用域的 Agent 调用，对不支持 project 的轨迹必须 fail-closed
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("lazy pool");
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("user-1", pool)
            .to_builder()
            .caller_type(CallerType::Agent)
            .project_id("project-1")
            .build();
        assert!(
            with_context_fields(
                ctx,
                TraceScopeFields {
                    agent_id: None,
                    project_id: None,
                    task_id: None,
                    limit: None,
                    max_limit: 100,
                    allow_project: false,
                    noun: "memory trace",
                },
            )
            .is_err()
        );
    }
}
