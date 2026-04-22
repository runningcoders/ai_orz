//! Tool call tracing - unified module for tool call logging and decorator wrapping
//!
//! This module provides:
//! - ToolCallEntry/ToolCallStatus: Structured logging entry with call status
//! - ToolCallLogger: Daily JSONL based logger for persistent tool call history
//! - tool_call_logger::ToolCallLogger: Logging decorator for our core Tool trait
//! 
//! Rig adapter moved to models::tool::RigToolAdapter now.

pub mod entry;
pub mod logger;
pub mod tool_call_logger;

#[cfg(test)]
mod logger_test;

pub use entry::{ToolCallEntry, ToolCallStatus};
pub use logger::ToolCallLogger;
pub use tool_call_logger::LoggingDecorator as ToolCallLoggingDecorator;
