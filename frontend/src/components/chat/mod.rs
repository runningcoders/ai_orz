//! 聊天共享组件
//!
//! - `MessageBubble`: 单条消息气泡（文本/图片/文件），简版渲染，不含工具卡片
//! - `TypingIndicator`: 输入指示器（三点动画）
//! - `MessageList`: 消息列表（含空状态 + typing 指示器）

pub mod message_bubble;
pub mod typing_indicator;

pub use message_bubble::MessageBubble;
pub use typing_indicator::TypingIndicator;
