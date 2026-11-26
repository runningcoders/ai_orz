//! 入站 SSE 响应基建
//!
//! SSE（`text/event-stream`）是 HTTP 响应的一种形态，与 [`super::file`] 并列：
//! 一个负责「把文件字节吐给浏览器」，一个负责「把事件流吐给浏览器」。
//!
//! 业务 handler 只声明三件事：**订阅谁、丢帧怎么表达、断开怎么清理**；
//! `BroadcastStream` 装配、心跳间隔、Drop 清理不再各写一遍。
//!
//! ⚠️ 本模块只管 HTTP 响应侧。事件源本身（如 `pkg::focus_live` 的按 Agent
//! 广播）仍属于各自的运行时基建，不并入这里。

use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::response::sse::{Event, KeepAlive};
use futures_util::Stream;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// SSE 心跳间隔（低于常见企业网关 60s idle timeout）
pub const KEEP_ALIVE_SECS: u64 = 15;

/// 统一心跳：15s 一次文本占位，避免代理把空闲连接掐断
pub fn keep_alive() -> KeepAlive {
    KeepAlive::new()
        .interval(std::time::Duration::from_secs(KEEP_ALIVE_SECS))
        .text("keep-alive")
}

/// 广播通道 → SSE 事件流
///
/// - `Ok(json)`：原样作为 `data` 下发（JSON 由业务侧序列化）
/// - `Lagged(n)`：交给 `on_lagged` 决定语义 —— 消息流用 [`ping_event`] 兜底，
///   专注流用 `focus.resync` 让前端重置图状态
pub fn event_stream<F>(
    rx: broadcast::Receiver<String>,
    on_lagged: F,
) -> impl Stream<Item = Result<Event, Infallible>> + Unpin
where
    F: Fn(u64) -> Event + Send + 'static,
{
    BroadcastStream::new(rx).filter_map(move |item| match item {
        Ok(data) => Some(Ok(Event::default().data(data))),
        Err(BroadcastStreamRecvError::Lagged(skipped)) => Some(Ok(on_lagged(skipped))),
    })
}

/// 丢帧兜底：业务流没有恢复语义时，发一条心跳占位（保持连接不断）
pub fn ping_event() -> Event {
    Event::default().event("ping").data("keep-alive")
}

/// SSE 连接清理包装
///
/// stream 被丢弃（客户端断开 / 服务关闭）时执行 `cleanup`。
/// 替代 `tokio::signal::ctrl_c()` 那种「等进程退出才清理」的写法 ——
/// 后者在客户端关闭浏览器时不会触发，订阅表会持续增长。
pub struct CleanupStream<S> {
    inner: S,
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl<S> CleanupStream<S> {
    pub fn new(inner: S, cleanup: impl FnOnce() + Send + 'static) -> Self {
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
