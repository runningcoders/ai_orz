//! Tool call tracing - unified module for tool call logging and decorator wrapping
//!
//! This module provides:
//! - ToolCallEntry/ToolCallStatus: Structured logging entry with call status
//! - ToolCallLogger: Daily JSONL based logger for persistent tool call history
//! - LoggingToolDecorator: Decorator for Rig-called built-in tools to auto-log invocations

pub mod entry;
pub mod logger;
pub mod decorator;

#[cfg(test)]
mod logger_test;

pub use entry::{ToolCallEntry, ToolCallStatus};
pub use logger::ToolCallLogger;
pub use decorator::LoggingToolDecorator;
