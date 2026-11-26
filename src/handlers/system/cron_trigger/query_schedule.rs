//! Handler：查询日程（Agent 工具 + HTTP 接口）。
//!
//! 本层只做三件事：身份从 ctx 固化、HTTP 参数映射到领域查询、DTO 透传。
//! 日期词解析 / 关键词过滤 / 规则展开 / 排序截断全部属于领域语义，
//! 在 `domain::system::schedule_query` 实现，consumer 可直接复用同一个方法。
//!
//! 与 `list_cron_triggers` 的区别：list 返回规则本身（一条重复规则只出现一次），
//! 本接口返回「某段时间里会发生什么」——`0 9 * * *` 查一周返回 7 条实例。

use crate::pkg::RequestContext;
use crate::service::domain::system::{ScheduleQuery, domain as system_domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{QueryScheduleRequest, QueryScheduleResponse};
use common::enums::ScheduleDateTerm;
use common::error::{Result, bail_err};

/// HTTP 请求 → 领域查询参数。
///
/// 身份刻意不在参数里：`query_schedules` 从 ctx 固化 owner，调用方无法伪造。
fn to_query(params: QueryScheduleRequest) -> Result<ScheduleQuery> {
    // 显式区间必须成对且有效——早失败在这里完成，domain 侧也有同等守卫
    match (params.from, params.to) {
        (Some(from), Some(to)) if to <= from => {
            bail_err!(InvalidRequest, "to 必须大于 from");
        }
        (Some(_), None) | (None, Some(_)) => {
            bail_err!(InvalidRequest, "from 与 to 必须同时提供");
        }
        _ => {}
    }
    Ok(ScheduleQuery {
        date_term: params.date_term.unwrap_or(ScheduleDateTerm::Today),
        from: params.from,
        to: params.to,
        keyword: params.keyword,
        behavior: params.behavior,
        is_enabled: params.is_enabled,
        // 接口约定默认 200、硬上限 500；领域内再 clamp 一次防绕过
        limit: params.limit.unwrap_or(200).min(500),
    })
}

/// 日程查询：按日期词或显式区间展开当前用户的日程规则。
///
/// 模型只需传 `date_term`（如 `tomorrow`），**不要自己算毫秒时间戳**；
/// 服务端按 `server.timezone` 折算区间，再把 cron/interval/once 规则展开成实例。
#[register_handler_tool(
    id = "query_schedule",
    name = "Query Schedule",
    description = "Query the caller's own schedules within a time range. Pass date_term (yesterday / today / tomorrow / this_week / next_week) to ask about relative days like tomorrow or next week — the server resolves calendar-day boundaries in the system timezone, so do NOT compute Unix timestamps yourself. For an exact range, pass from and to together as Unix milliseconds. Optional keyword filters by title/description/label, behavior filters by notification vs agent-wakeup, is_enabled filters state. Returns individual occurrences sorted by time: a daily repeating schedule queried for a week returns seven entries. Only schedules owned by the calling user are visible.",
    params = "common::api::QueryScheduleRequest",
    neural,
    tags = "query,scheduling,autonomy"
)]
#[generate_http_handler]
pub async fn query_schedule(
    ctx: RequestContext,
    params: QueryScheduleRequest,
) -> Result<QueryScheduleResponse> {
    let query = to_query(params)?;
    system_domain()
        .cron_manager()
        .query_schedules(ctx, query)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_defaults_to_domain_query() {
        let q = to_query(QueryScheduleRequest::default()).unwrap();
        assert_eq!(q.date_term, ScheduleDateTerm::Today);
        assert_eq!(q.limit, 200);
    }

    #[test]
    fn rejects_partial_or_inverted_range() {
        let partial = QueryScheduleRequest {
            from: Some(100),
            ..Default::default()
        };
        assert!(to_query(partial).is_err());

        let inverted = QueryScheduleRequest {
            from: Some(200),
            to: Some(100),
            ..Default::default()
        };
        assert!(to_query(inverted).is_err());
    }

    #[test]
    fn clamps_limit_to_hard_cap() {
        let q = to_query(QueryScheduleRequest {
            limit: Some(9999),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.limit, 500);
    }
}
