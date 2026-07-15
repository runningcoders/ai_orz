//! Handler: GET /api/v1/finance/messages/sse - SSE 消息推送订阅
//!
//! 从 JWT 认证信息中获取当前用户 ID，无需路径参数传递
//! 浏览器 EventSource 自动携带 Cookie，认证由 JWT 中间件完成

use axum::response::sse::{Event, Sse};
use axum::Extension;
use futures_util::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use crate::pkg::RequestContext;
use crate::service::domain::message;

/// SSE 订阅端点
pub async fn subscribe_sse_handler(
    Extension(ctx): Extension<RequestContext>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let user_id = ctx.uid().to_string();
    let subscribe_result = message::domain()
        .delivery()
        .subscribe_sse(ctx.clone(), &user_id)
        .await
        .unwrap();

    let connection_id = subscribe_result.connection_id.clone();
    let rx = subscribe_result.receiver;

    let stream = BroadcastStream::new(rx)
        .map(move |msg| {
            match msg {
                Ok(data) => Event::default().data(data),
                Err(_) => Event::default().event("ping").data("keep-alive"),
            }
        })
        .map(Ok::<_, Infallible>);

    let ctx_clone = ctx.clone();
    let conn_id_clone = connection_id.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = message::domain()
            .delivery()
            .unsubscribe_sse(ctx_clone, &conn_id_clone)
            .await;
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}
