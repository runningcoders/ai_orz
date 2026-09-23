//! 聊天共享组件
//!
//! - `MessageBubble`: 单条消息气泡（文本/图片/文件），简版渲染，不含工具卡片
//! - `TypingIndicator`: 输入指示器（三点动画）
//! - `MessageList`: 消息列表（含空状态 + typing 指示器）
//! - `ChatSidePanel`: 聊天信息侧栏（总览/任务/产物/Agent/我/工具 多 Tab，只读）
//! - `ToolCallsTab`: 工具调用记录 Tab（call_id join 运行中进程，行头带发起 Agent 身份 chip）
//! - `ProjectAgentsTab`: 项目模式 Agent Tab —— 项目内 Agent 列表（assignee 推导）
//!   与单 Agent 详情子页（复用 `AgentInfoTab`），面板内视图切换不走路由

pub mod chat_side_panel;
pub mod message_bubble;
pub mod project_agents_tab;
pub mod tool_calls_tab;
pub mod typing_indicator;

pub use chat_side_panel::ChatSidePanel;
pub use message_bubble::MessageBubble;
pub use project_agents_tab::ProjectAgentsTab;
pub use tool_calls_tab::ToolCallsTab;
pub use typing_indicator::TypingIndicator;
