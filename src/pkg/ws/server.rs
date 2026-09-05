//! WS 服务端会话循环（pkg 通用组件）
//!
//! 与 client 侧对称：心跳、读循环、优雅关闭由 pkg 承载；帧语义由
//! `WsServerHandler` 实现方全权解释。服务端被动接受连接，**无 supervisor
//! 重连**（断开即结束，对端负责重拨）；握手鉴权在 upgrade 前由调用方完成。
//!
//! 用法：
//! ```ignore
//! // axum upgrade handler 内（鉴权后）：
//! ws::server::serve(socket, Arc::new(MyHandler)).await;
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message as AxumMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::Mutex;

use common::error::{Result, err};

use super::FrameTx;

/// 心跳间隔（与 client 一致，低于常见企业代理 60s idle timeout）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 单帧处置动作（与 client 侧同义）
pub use super::FrameAction;

/// WS 服务端会话处理器：帧语义由实现方全权解释
#[async_trait]
pub trait WsServerHandler: Send + Sync {
    /// 组件名（日志标识）
    fn name(&self) -> &str;

    /// 连接建立后回调（注册出站句柄用）
    async fn on_connected(&self, _tx: Arc<dyn FrameTx>) {}

    /// 处理一帧文本。返回 `FrameAction::Reconnect` 表示应关闭本连接
    /// （服务端无重连语义，等同 close；由对端决定是否重拨）。
    async fn on_frame(&self, text: String, tx: Arc<dyn FrameTx>) -> FrameAction;

    /// 连接关闭后回调（注销会话用）
    async fn on_closed(&self) {}
}

/// 运行一次服务端会话直到连接关闭
///
/// 调用方在 axum upgrade handler 内完成握手鉴权后调用；
/// 本函数阻塞至连接结束（读循环内不做业务——`on_frame` 实现方自行
/// 入队/异步消费）。
pub async fn serve(socket: WebSocket, handler: Arc<dyn WsServerHandler>) {
    let name = handler.name().to_string();
    let (write, mut read) = socket.split();
    let write = Arc::new(Mutex::new(write));

    let tx: Arc<dyn FrameTx> = Arc::new(ServerFrameTx {
        write: write.clone(),
        closed: AtomicBool::new(false),
    });
    handler.on_connected(tx.clone()).await;

    // 心跳任务：协议级 Ping（应用层心跳语义在信封里，不需要自定义帧）
    let heartbeat_write = write.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            ticker.tick().await;
            let mut w = heartbeat_write.lock().await;
            if let Err(e) = w.send(AxumMessage::Ping(Vec::new().into())).await {
                log_warn!("ws server heartbeat send failed: {}", e);
                break;
            }
        }
    });

    // 接收循环
    loop {
        match read.next().await {
            Some(Ok(msg)) => match msg {
                AxumMessage::Text(text) => {
                    if handler.on_frame(text.to_string(), tx.clone()).await
                        == FrameAction::Reconnect
                    {
                        log_info!("{} ws server handler requested close", name);
                        break;
                    }
                }
                AxumMessage::Close(_) => {
                    log_info!("{} ws server received close frame", name);
                    break;
                }
                // Ping/Pong/二进制帧：忽略（协议层 Pong 由底层自动应答）
                _ => {}
            },
            Some(Err(e)) => {
                log_error!("{} ws server recv error: {}", name, e);
                break;
            }
            None => {
                log_info!("{} ws server stream closed", name);
                break;
            }
        }
    }

    let _ = tx.close().await;
    heartbeat_handle.abort();
    let _ = heartbeat_handle.await;
    handler.on_closed().await;
    log_info!("{} ws server session exited", name);
}

/// server 端帧发送句柄（内部包 axum 写端）
struct ServerFrameTx {
    write: Arc<Mutex<futures_util::stream::SplitSink<WebSocket, AxumMessage>>>,
    closed: AtomicBool,
}

#[async_trait]
impl FrameTx for ServerFrameTx {
    async fn send_text(&self, text: String) -> Result<()> {
        let mut w = self.write.lock().await;
        w.send(AxumMessage::Text(text.into()))
            .await
            .map_err(|e| err!(ThirdPartyError, "ws server send error: {}", e))
    }

    async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::SeqCst);
        let mut w = self.write.lock().await;
        let _ = w.close().await;
        Ok(())
    }

    fn is_alive(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
}
