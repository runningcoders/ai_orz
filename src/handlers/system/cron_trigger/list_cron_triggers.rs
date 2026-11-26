//! Handler: GET /api/v1/system/cron-triggers - List Cron Triggers.

use ai_orz_macros::generate_http_handler;
use common::enums::TriggerStatus;
use common::error::Result;

use crate::pkg::RequestContext;
use crate::service::dao::cron_trigger::CronTriggerQuery;
use crate::service::domain::system::domain;

use super::response::{CronTriggerDetail, ListCronTriggersRequest, ListCronTriggersResponse};

/// 管理端默认每页 50 条、硬上限 200；状态缺省只看启用，防止规模增长后误拉全量。
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

fn to_query(params: &ListCronTriggersRequest) -> CronTriggerQuery {
    let statuses = params
        .statuses
        .clone()
        .unwrap_or_else(|| vec![TriggerStatus::Enabled]);
    CronTriggerQuery {
        trigger_type: params.trigger_type,
        behaviors: params.behaviors.clone().unwrap_or_default(),
        statuses: if statuses.is_empty() {
            vec![TriggerStatus::Enabled]
        } else {
            statuses
        },
        offset: params.offset,
        limit: Some(params.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT)),
        ..Default::default()
    }
}

#[generate_http_handler]
pub async fn list_cron_triggers(
    ctx: RequestContext,
    params: ListCronTriggersRequest,
) -> Result<ListCronTriggersResponse> {
    // 过滤与分页全部下推数据端；total 是过滤后的总行数，不是当前页条数。
    let query = to_query(&params);
    let domain = domain();
    let manager = domain.cron_manager();
    let total = manager.count_triggers(ctx.clone(), query.clone()).await?;
    let triggers = manager.list_triggers(ctx, query).await?;

    let triggers: Vec<CronTriggerDetail> =
        triggers.iter().map(super::response::to_detail).collect();

    Ok(ListCronTriggersResponse { triggers, total })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_enabled_and_paged() {
        let query = to_query(&ListCronTriggersRequest::default());
        assert_eq!(query.statuses, vec![TriggerStatus::Enabled]);
        assert_eq!(query.offset, None);
        assert_eq!(query.limit, Some(DEFAULT_LIMIT));
        assert!(query.behaviors.is_empty());
    }

    #[test]
    fn explicit_status_list_and_limit_are_kept() {
        let req = ListCronTriggersRequest {
            statuses: Some(vec![TriggerStatus::Enabled, TriggerStatus::Disabled]),
            behaviors: None,
            offset: Some(20),
            limit: Some(999),
            ..Default::default()
        };
        let query = to_query(&req);
        assert_eq!(query.statuses.len(), 2);
        assert_eq!(query.offset, Some(20));
        assert_eq!(query.limit, Some(MAX_LIMIT));
    }
}
