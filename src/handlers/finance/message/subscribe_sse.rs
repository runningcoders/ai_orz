//! Handler: GET /api/v1/finance/messages/sse - SSE 消息推送订阅
//!
//! 从 JWT 认证信息中获取当前用户 ID，无需路径参数传递
//! 浏览器 EventSource 自动携带 Cookie，认证由 JWT 中间件完成

use axum::response::sse::{Event, Sse};
use axum::Extension;
use futures_util::Stream;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use crate::pkg::RequestContext;
use crate::service::domain::message;

/// 包装 SSE stream，在 stream 被丢弃时（客户端断开或服务关闭）自动注销连接。
///
/// 修复：之前清理逻辑等待 ctrl_c 信号，客户端关闭浏览器时不会触发清理，
/// connections 和 user_connections map 无限增长（内存泄漏）。
/// 现在 stream 被丢弃时 Drop guard 触发异步注销连接。
struct CleanupStream<S> {
    inner: S,
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl<S> CleanupStream<S> {
    fn new(inner: S, cleanup: impl FnOnce() + Send + 'static) -> Self {
        Self {
            inner,
            cleanup: Some(Box::new(cleanup)),
        }
    }
}

impl<S: Stream + Unpin> Stream for CleanupStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl<S> Drop for CleanupStream<S> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

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

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}
