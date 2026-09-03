//! 脱敏引擎路径兼容层
//!
//! 实现已下沉至 [`common::redaction`]（前后端共享，单一事实源），本模块仅作
//! 路径转发，保证后端既有 `crate::pkg::redaction::...` 引用继续可用。
//! `redact!` 宏由 `src/lib.rs` 经 `pub use common::redact;` 转发为 `crate::redact!`。

pub use common::redaction::*;
