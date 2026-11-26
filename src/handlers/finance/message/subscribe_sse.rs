//! Handler: GET /api/v1/finance/messages/sse - SSE 消息推送订阅
//!
//! 从 JWT 认证信息中获取当前用户 ID，无需路径参数传递
//! 浏览器 EventSource 自动携带 Cookie，认证由 JWT 中间件完成
//!
//! 响应装配（心跳 / 丢帧 / 断开清理）统一走 `pkg::http::sse`；
//! 本 handler 只声明「订阅当前用户、丢帧转心跳、断开注销连接」。

use crate::pkg::RequestContext;
use crate::pkg::http::sse::{CleanupStream, event_stream, keep_alive, ping_event};
use crate::service::domain::message;
use axum::Extension;
use axum::response::sse::{Event, Sse};
use futures_util::Stream;
use std::convert::Infallible;

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

    // 消息流没有恢复语义：丢帧时发一条心跳占位，保持连接不断
    let stream = event_stream(rx, |_| ping_event());

    // 客户端断开或服务关闭时，stream 被丢弃 → Drop guard 触发注销连接
    let ctx_clone = ctx.clone();
    let conn_id_clone = connection_id.clone();
    let stream = CleanupStream::new(stream, move || {
        let ctx = ctx_clone;
        let conn_id = conn_id_clone;
        tokio::spawn(async move {
            let _ = message::domain()
                .delivery()
                .unsubscribe_sse(ctx, &conn_id)
                .await;
        });
    });

    Sse::new(stream).keep_alive(keep_alive())
}
