//! 聊天共享组件
//!
//! - `MessageBubble`: 单条消息气泡（文本/图片/文件），简版渲染，不含工具卡片
//! - `TypingIndicator`: 输入指示器（三点动画）
//! - `MessageList`: 消息列表（含空状态 + typing 指示器）
//! - `ChatSidePanel`: 聊天信息侧栏（项目模式：总览/任务/产物/Agent/工具；默认对话：Agent/任务/产物/工具，只读）
//! - `ToolCallsTab`: 工具调用记录 Tab（call_id join 运行中进程，行头带发起 Agent 身份 chip）
//! - `ProjectAgentsTab`: 项目模式 Agent Tab —— 项目内 Agent 列表（assignee 推导）
//!   与单 Agent 详情子页（复用 `AgentInfoTab`），面板内视图切换不走路由
//! - `CancelThinkingButton`: 「停止思考」按钮（含二次确认），对话内取消目标 Agent
//!   当前思考轮次；置底状态气泡与输入区发送键位两个落点共用

pub mod cancel_thinking_button;
pub mod chat_side_panel;
pub mod message_bubble;
pub mod project_agents_tab;
pub mod tool_calls_tab;
pub mod typing_indicator;

pub use cancel_thinking_button::{CancelThinkingButton, CancelThinkingStyle};
pub use chat_side_panel::ChatSidePanel;
pub use message_bubble::MessageBubble;
pub use project_agents_tab::ProjectAgentsTab;
pub use tool_calls_tab::ToolCallsTab;
pub use typing_indicator::TypingIndicator;
