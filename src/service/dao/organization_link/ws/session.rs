//! 联邦 WS 会话：帧路由 / 服务端 handler / 客户端拨号
//!
//! 入站帧路由（server 会话与 client adapter 共用同一逻辑）：
//! - `response` 帧 → 截获唤醒 pending 表（不进事件总线）
//! - 命令帧 → publish `FederationInboundEvent` → 业务 consumer 异步消费
//!
//! 鉴权前移连接层：server 端在 upgrade 前由 handler 调
//! `resolve_federation_identity` 完成（凭证鉴权 + 能力门禁），重连必须
//! 重新握手（P0）。入站命令的业务 ctx 由 consumer 按事件经 domain 解析
//! （本端接待用户 + 对端 caller_org）；session 只持有两端组织 ID 对，
//! 不持有任何用户身份。

use std::sync::Arc;

use async_trait::async_trait;
use common::error::err;
use serde_json::Value;

use crate::models::events::{FederationFrame, FederationInboundEvent};
use crate::pkg::RequestContext;
use crate::pkg::aop;
use crate::pkg::ws::{FrameAction, FrameTx, WsClientAdapter, WsClientState, WsServerHandler};

use super::connection::registry;
use super::pending::pending;

// ==================== 帧路由（server/client 共用） ====================

/// 处理一帧对端消息
///
/// - 解析失败：忽略（Continue，坏帧不断连接）
/// - 响应帧：唤醒 pending 表
/// - 命令帧：publish 入站事件（业务 ctx 由 consumer 解析，事件信封零身份）
pub async fn route_frame(local_org: &str, peer_org: &str, text: String) -> FrameAction {
    let frame: FederationFrame = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            log_debug!(
                "federation ws parse frame failed (ignored): peer={} err={} body={}",
                peer_org,
                e,
                text
            );
            return FrameAction::Continue;
        }
    };

    if frame.is_response() {
        let woken = pending().resolve(&frame.correlation_id, Ok(frame.payload));
        if !woken {
            log_debug!(
                "federation ws late response (no pending): peer={} correlation_id={}",
                peer_org,
                frame.correlation_id
            );
        }
        return FrameAction::Continue;
    }

    log_info!(
        "federation ws inbound command: peer={} kind={} correlation_id={}",
        peer_org,
        frame.kind,
        frame.correlation_id
    );
    let ctx = RequestContext::new_system();
    aop::registry()
        .publish(
            &ctx,
            FederationInboundEvent {
                local_org: local_org.to_string(),
                peer_org: peer_org.to_string(),
                frame,
            },
        )
        .await;
    FrameAction::Continue
}

// ==================== server 端会话 ====================

/// 联邦 WS 服务端会话（对端拨入本端时的读循环 handler）
pub struct FederationWsSession {
    local_org: String,
    peer_org: String,
}

impl FederationWsSession {
    pub fn new(local_org: String, peer_org: String) -> Self {
        Self {
            local_org,
            peer_org,
        }
    }
}

#[async_trait]
impl WsServerHandler for FederationWsSession {
    fn name(&self) -> &str {
        "federation_ws"
    }

    /// 建连即注册（对端沿此连接反向可达）
    async fn on_connected(&self, tx: Arc<dyn FrameTx>) {
        registry().register(&self.peer_org, tx);
        log_info!("federation ws session registered: peer={}", self.peer_org);
    }

    async fn on_frame(&self, text: String, _tx: Arc<dyn FrameTx>) -> FrameAction {
        route_frame(&self.local_org, &self.peer_org, text).await
    }

    async fn on_closed(&self) {
        log_info!("federation ws session closed: peer={}", self.peer_org);
    }
}

// ==================== client 端拨号 ====================

/// 联邦 WS 客户端 adapter（本端拨出）
pub struct FederationWsClientAdapter {
    local_org: String,
    peer_org: String,
    url: String,
    token: String,
    caller_declaration: Option<String>,
}

#[async_trait]
impl WsClientAdapter for FederationWsClientAdapter {
    fn name(&self) -> &str {
        "federation_ws"
    }

    async fn endpoint(&self) -> common::error::Result<String> {
        Ok(self.url.clone())
    }

    async fn on_frame(&self, text: String) -> FrameAction {
        route_frame(&self.local_org, &self.peer_org, text).await
    }

    /// Bearer 凭证 + 身份声明注入握手请求；重连时重新调用（重新握手鉴权）
    fn handshake_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![("Authorization", format!("Bearer {}", self.token))];
        if let Some(decl) = &self.caller_declaration {
            headers.push((
                common::constants::http_header::FEDERATION_CALLER,
                decl.clone(),
            ));
        }
        headers
    }
}

/// client 侧出站句柄代理（registry 统一存 FrameTx；client 重连后 tx 随 state 更新）
struct StateFrameTx {
    state: Arc<WsClientState>,
}

#[async_trait]
impl FrameTx for StateFrameTx {
    async fn send_text(&self, text: String) -> common::error::Result<()> {
        self.state.send_text(text).await
    }

    async fn close(&self) -> common::error::Result<()> {
        // 客户端连接关闭交给 supervisor（shutdown），这里仅标记
        Ok(())
    }

    fn is_alive(&self) -> bool {
        // 同步尽力判断（try_read 锁被占用时 false，send_text 会做最终校验）
        self.state.try_is_connected()
    }
}

/// 由任意对端地址推导 WS 端点 URL
///
/// - 剥离遗留 A2A 路径后缀（如 `http://host/a2a`）
/// - scheme 转换：`http://` → `ws://`，`https://` → `wss://`
///   （tungstenite 的 `connect_async` 只接受 ws/wss scheme）
pub fn ws_url_from_base(endpoint: &str) -> String {
    const FEDERATION_WS_PATH: &str = "/api/v1/organization/links/ws";
    let mut base = endpoint.trim().trim_end_matches('/').to_string();
    if base.ends_with("/a2a") {
        base.truncate(base.len() - 4);
    }
    if let Some(rest) = base.strip_prefix("https://") {
        base = format!("wss://{}", rest);
    } else if let Some(rest) = base.strip_prefix("http://") {
        base = format!("ws://{}", rest);
    }
    format!("{}{}", base, FEDERATION_WS_PATH)
}

/// 拨出对端并注册会话（url 已由调用方经 P7 resolver 解析；凭证取 link 明文）
///
/// - `local_org` / `peer_org`：两端组织 ID（入站事件按此注入接待模型）
/// - `caller_declaration`：本端身份声明（X-Federation-Caller），随**握手**一次注入，
///   之后帧信封内绝不携带身份（P0 红线）——重连重新握手即重新声明
///
/// 返回 client state（supervisor 自动重连；重连即重新握手鉴权）。
pub async fn dial_peer(
    local_org: &str,
    peer_org: &str,
    url: String,
    token: String,
    caller_declaration: Option<String>,
) -> common::error::Result<Arc<WsClientState>> {
    let adapter = Arc::new(FederationWsClientAdapter {
        local_org: local_org.to_string(),
        peer_org: peer_org.to_string(),
        url,
        token,
        caller_declaration,
    });
    let state = Arc::new(crate::pkg::ws::start_client(adapter).await?);
    registry().register(
        peer_org,
        Arc::new(StateFrameTx {
            state: state.clone(),
        }),
    );
    log_info!("federation ws dialed: peer={}", peer_org);
    Ok(state)
}

// ==================== 出站原语 ====================

/// 向对端 push 一帧（无活连接时报错，由调用方决定回退策略）
pub async fn push_frame(peer_org: &str, frame: &FederationFrame) -> common::error::Result<()> {
    let Some(tx) = registry().lookup(peer_org) else {
        return Err(err!(
            ThirdPartyError,
            "federation ws not connected: peer={}",
            peer_org
        ));
    };
    let text = serde_json::to_string(frame)
        .map_err(|e| err!(Internal, "federation frame serialize failed: {}", e))?;
    tx.send_text(text).await
}

/// 经 WS 向对端发起命令并等待响应（注册 pending → push → 等待）
///
/// 供上层（`call_peer` facade）组合 HTTP 回退；本函数只负责 WS 通道。
pub async fn request_over_ws(
    peer_org: &str,
    kind: &str,
    correlation_id: String,
    payload: Value,
) -> common::error::Result<Value> {
    // 先查活连接再注册 pending，避免对无连接请求挂起 30s
    if !registry().connected(peer_org) {
        return Err(err!(
            ThirdPartyError,
            "federation ws not connected: peer={}",
            peer_org
        ));
    }
    let rx = pending().register(&correlation_id);
    let frame = FederationFrame::command(kind, correlation_id.clone(), payload);
    if let Err(e) = push_frame(peer_org, &frame).await {
        // 出站失败立即清理，不悬挂
        pending().remove(&correlation_id);
        return Err(e);
    }
    pending()
        .wait(
            &correlation_id,
            rx,
            std::time::Duration::from_secs(pending().default_timeout_secs),
        )
        .await
}
