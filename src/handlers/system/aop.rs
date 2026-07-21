//! AOP 队列监控 HTTP 接口

use axum::{
    Json,
    extract::{Path, Query},
};
use common::api::ApiResponse;

use crate::service::domain::system::domain;

#[derive(Debug, serde::Deserialize)]
pub struct EventListQuery {
    pub order_key: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

#[derive(Debug, serde::Serialize)]
pub struct QueueStatsResponse {
    pub consumer_name: String,
    pub pending_count: usize,
    pub in_progress_count: usize,
    pub order_keys: Vec<OrderKeyInfo>,
    pub oldest_event_age_secs: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct OrderKeyInfo {
    pub order_key: String,
    pub pending_count: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct EventSummaryResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
}

#[derive(Debug, serde::Serialize)]
pub struct EventDetailResponse {
    pub event_id: String,
    pub event_kind: String,
    pub order_key: String,
    pub priority: u8,
    pub created_at: i64,
    pub status: String,
    pub payload_preview: String,
}

/// GET /api/v1/system/aop/stats
pub async fn get_all_queue_stats() -> Json<ApiResponse<Vec<QueueStatsResponse>>> {
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

    Json(ApiResponse::success(response))
}

/// GET /api/v1/system/aop/{consumer}/stats
pub async fn get_queue_stats(
    Path(consumer): Path<String>,
) -> Json<ApiResponse<QueueStatsResponse>> {
    let stats = domain().aop_monitor().queue_stats(&consumer);

    match stats {
        Some(s) => Json(ApiResponse::success(QueueStatsResponse {
            consumer_name: consumer,
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
        })),
        None => Json(ApiResponse {
            code: 404,
            message: format!("Consumer queue '{}' not found", consumer),
            data: None,
        }),
    }
}

/// GET /api/v1/system/aop/{consumer}/events
pub async fn list_events(
    Path(consumer): Path<String>,
    Query(query): Query<EventListQuery>,
) -> Json<ApiResponse<Vec<EventSummaryResponse>>> {
    let status = query.status.and_then(|s| match s.to_lowercase().as_str() {
        "pending" => Some(crate::pkg::aop::queue::EventStatus::Pending),
        "processing" => Some(crate::pkg::aop::queue::EventStatus::Processing),
        _ => None,
    });

    let filter = crate::pkg::aop::queue::EventQueryFilter {
        order_key: query.order_key,
        status,
        limit: query.limit.min(1000),
        offset: query.offset,
    };

    let events = domain().aop_monitor().list_events(&consumer, filter);

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
            Json(ApiResponse::success(response))
        }
        None => Json(ApiResponse {
            code: 404,
            message: format!("Consumer queue '{}' not found", consumer),
            data: None,
        }),
    }
}

/// GET /api/v1/system/aop/{consumer}/events/{event_id}
pub async fn get_event(
    Path((consumer, event_id)): Path<(String, String)>,
) -> Json<ApiResponse<EventDetailResponse>> {
    let event = domain().aop_monitor().get_event(&consumer, &event_id);

    match event {
        Some(e) => Json(ApiResponse::success(EventDetailResponse {
            event_id: e.summary.event_id,
            event_kind: e.summary.event_kind,
            order_key: e.summary.order_key,
            priority: e.summary.priority,
            created_at: e.summary.created_at,
            status: format!("{:?}", e.summary.status).to_lowercase(),
            payload_preview: e.payload_preview,
        })),
        None => Json(ApiResponse {
            code: 404,
            message: format!("Event '{}' not found in queue '{}'", event_id, consumer),
            data: None,
        }),
    }
}
