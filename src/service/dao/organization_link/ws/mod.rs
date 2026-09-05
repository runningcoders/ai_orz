//! 联邦 WS 长连接 adapter（dao 层有状态组件）
//!
//! 职责：帧 ⇄ 事件转换、连接注册表、请求-响应配对、客户端拨号。
//! 服务端 upgrade 入口在 `handlers/organization/links/federation_ws`，
//! 鉴权复用 `resolve_federation_identity`（连接层一次，粒度 session）。
//!
//! 事件链路（与 lark 样板同构）：
//! - 入站：`route_frame` → publish `FederationInboundEvent` → 业务 consumer
//! - 出站：业务 publish `FederationOutboundEvent` → 出站 consumer 查注册表 push
//! - 响应：session 截获 `response` 帧 → pending 表唤醒（不进事件总线）

pub mod connection;
pub mod pending;
pub mod session;

pub use connection::registry;
pub use pending::pending;
pub use session::{FederationWsSession, dial_peer, push_frame, request_over_ws, ws_url_from_base};
