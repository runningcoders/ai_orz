//! AopStats 子能力实现
//!
//! 直接读取全局 AopStatsCollector 内存快照，无 DAO/DAL 中转。

use async_trait::async_trait;
use common::error::{Error, Result};

use crate::consumer::{AopDistributionItem, AopOverview, AopTimeSeriesPoint};
use crate::pkg::RequestContext;

use super::aop_stats_collector;
use super::{AopStats, SystemDomainImpl};

#[async_trait]
impl AopStats for SystemDomainImpl {
    async fn overview(&self, _ctx: RequestContext) -> Result<AopOverview> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector.overview().await)
    }

    async fn time_series(
        &self,
        _ctx: RequestContext,
        event_kind: Option<String>,
        consumer_name: Option<String>,
        status: Option<String>,
    ) -> Result<Vec<AopTimeSeriesPoint>> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector
            .time_series(
                event_kind.as_deref(),
                consumer_name.as_deref(),
                status.as_deref(),
            )
            .await)
    }

    async fn distribution(
        &self,
        _ctx: RequestContext,
        group_by: String,
        status_filter: Option<String>,
    ) -> Result<Vec<AopDistributionItem>> {
        let collector = aop_stats_collector().ok_or_else(|| {
            Error::internal("AopStatsCollector not initialized")
        })?;
        Ok(collector
            .distribution(&group_by, status_filter.as_deref())
            .await)
    }
}
