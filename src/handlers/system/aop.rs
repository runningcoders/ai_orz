//! AOP 队列监控 HTTP 接口

use ai_orz_macros::generate_http_handler;
use common::api::{
    EventDetailResponse, EventSummaryResponse, GetAllQueueStatsRequest, GetEventRequest,
    GetQueueStatsRequest, ListEventsRequest, OrderKeyInfo, QueueStatsResponse,
};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::pkg::aop::queue::{EventQueryFilter, EventStatus};
use crate::service::domain::system::domain;

/// GET /api/v1/system/aop/stats
#[generate_http_handler]
pub async fn get_all_queue_stats(
    _ctx: RequestContext,
    _params: GetAllQueueStatsRequest,
) -> Result<Vec<QueueStatsResponse>> {
    let stats = domain().aop_monitor().all_queue_stats();

    let response: Vec<QueueStatsResponse> = stats
        .into_iter()
        .map(|(name, s)| QueueStatsResponse {
            consumer_name: name,
            pending_count: s.pending_count,
            in_progress_count: s.in_progress_count,
            order_keys: s
                .order_keys
                .into_iter()
                .map(|ok| OrderKeyInfo {
                    order_key: ok.order_key,
                    pending_count: ok.pending_count,
                })
                .collect(),
            oldest_event_age_secs: s.oldest_event_age_secs,
        })
        .collect();

    Ok(response)
}

/// GET /api/v1/system/aop/{consumer}/stats
#[generate_http_handler]
pub async fn get_queue_stats(
    _ctx: RequestContext,
    params: GetQueueStatsRequest,
) -> Result<QueueStatsResponse> {
    let stats = domain().aop_monitor().queue_stats(&params.consumer);

    match stats {
        Some(s) => Ok(QueueStatsResponse {
            consumer_name: params.consumer,
            pending_count: s.pending_count,
            in_progress_count: s.in_progress_count,
            order_keys: s
                .order_keys
                .into_iter()
                .map(|ok| OrderKeyInfo {
                    order_key: ok.order_key,
                    pending_count: ok.pending_count,
                })
                .collect(),
            oldest_event_age_secs: s.oldest_event_age_secs,
        }),
        None => Err(Error::not_found(format!(
            "Consumer queue '{}' not found",
            params.consumer
        ))),
    }
}

/// GET /api/v1/system/aop/{consumer}/events
#[generate_http_handler]
pub async fn list_events(
    _ctx: RequestContext,
    params: ListEventsRequest,
) -> Result<Vec<EventSummaryResponse>> {
    let status = params
        .status
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(EventStatus::Pending),
            "processing" => Some(EventStatus::Processing),
            _ => None,
        });

    let filter = EventQueryFilter {
        order_key: params.order_key,
        status,
        limit: params.limit.unwrap_or(100).min(1000),
        offset: params.offset.unwrap_or(0),
    };

    let events = domain().aop_monitor().list_events(&params.consumer, filter);

    match events {
        Some(list) => {
            let response: Vec<EventSummaryResponse> = list
                .into_iter()
                .map(|e| EventSummaryResponse {
                    event_id: e.event_id,
                    event_kind: e.event_kind,
                    order_key: e.order_key,
                    priority: e.priority,
                    created_at: e.created_at,
                    status: format!("{:?}", e.status).to_lowercase(),
                })
                .collect();
            Ok(response)
        }
        None => Err(Error::not_found(format!(
            "Consumer queue '{}' not found",
            params.consumer
        ))),
    }
}

/// GET /api/v1/system/aop/{consumer}/events/{event_id}
#[generate_http_handler]
pub async fn get_event(
    _ctx: RequestContext,
    params: GetEventRequest,
) -> Result<EventDetailResponse> {
    let event = domain()
        .aop_monitor()
        .get_event(&params.consumer, &params.event_id);

    match event {
        Some(e) => Ok(EventDetailResponse {
            event_id: e.summary.event_id,
            event_kind: e.summary.event_kind,
            order_key: e.summary.order_key,
            priority: e.summary.priority,
            created_at: e.summary.created_at,
            status: format!("{:?}", e.summary.status).to_lowercase(),
            payload_preview: e.payload_preview,
        }),
        None => Err(Error::not_found(format!(
            "Event '{}' not found in queue '{}'",
            params.event_id, params.consumer
        ))),
    }
}
