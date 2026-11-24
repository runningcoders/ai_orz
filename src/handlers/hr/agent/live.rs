//! Handler: GET /api/v1/hr/agents/{id}/live - 专注浮层实时思考图
//!
//! 纯订阅端：AOP 消费者已把思考骨架 / 细节事件投影成客户端可见字段，并按
//! `agent_id` 写入进程内 broadcast；这里订阅对应 Agent 的通道后转 SSE。
//!
//! 响应装配（心跳 / 丢帧 / 清理）统一走 `pkg::http::sse`；
//! 本 handler 只声明「丢帧必须 resync」这一业务语义。
//!
//! ⚠️ 无回放：客户端晚连会错过已发生的事件；历史由思考轨迹接口承担。
//! ⚠️ Agent 归属校验当前未做（与既有只读接口同口径，用户已确认暂缓）。

use axum::Extension;
use axum::extract::Path;
use axum::response::sse::{Event, Sse};
use futures_util::Stream;
use std::convert::Infallible;

use crate::pkg::RequestContext;
use crate::pkg::http::sse::{event_stream, keep_alive};

/// SSE 订阅端点
pub async fn live_handler(
    Extension(_ctx): Extension<RequestContext>,
    Path(agent_id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = crate::pkg::focus_live::subscribe(&agent_id);

    // Lagged：缓冲溢出导致中间事件丢失。继续订阅，但先让前端重置图状态，
    // 否则残缺的事件序列会把「旧的 active 节点」永久留在图上。
    let stream = event_stream(rx, |skipped| {
        let payload = serde_json::json!({
            "kind": "focus.resync",
            "skipped": skipped,
        })
        .to_string();
        Event::default().data(payload)
    });

    Sse::new(stream).keep_alive(keep_alive())
}
