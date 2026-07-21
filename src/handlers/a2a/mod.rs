//! A2A Protocol Server Handler
//!
//! 对外暴露 A2A 协议（JSON-RPC 2.0）端点，外部 A2A Client 可通过协议调用前台 Agent。
//!
//! 模块结构（按 Task 进度逐步添加）：
//! - `mapper` — A2A ↔ ai_orz 实体转换（Task 4）
//! - `agent_card` — Agent Card 发现端点（Task 5）
//! - `send_task` — tasks/send 异步提交（Task 6）
//! - `send_subscribe` — tasks/sendSubscribe SSE 流式（P2）
//! - `get_task` — tasks/get 查询（Task 7）
//! - `cancel_task` — tasks/cancel 取消（Task 8）
//! - `jsonrpc` — JSON-RPC 入口 + 方法分发（Task 9）

pub mod agent_card;
pub mod callback;
pub mod cancel_task;
pub mod get_task;
pub mod jsonrpc;
pub mod mapper;
pub mod send_task;
pub mod send_subscribe;

#[cfg(test)]
mod integration_test;

#[cfg(test)]
mod mapper_test;
