//! 聊天共享组件
//!
//! - `MessageBubble`: 单条消息气泡（文本/图片/文件），简版渲染，不含工具卡片
//! - `TypingIndicator`: 输入指示器（三点动画）
//! - `MessageList`: 消息列表（含空状态 + typing 指示器）
//! - `ChatSidePanel`: 聊天信息侧栏（项目模式：总览/任务/产物/Agent/工具/思考；默认对话：Agent/任务/产物/工具/思考，只读）
//! - `ToolCallsTab`: 工具调用记录 Tab（call_id join 运行中进程，行头带发起 Agent 身份 chip）
//! - `ThinkTracesTab`: 思考轨迹 Tab（pkg/think_trace 分页查询，Agent 详情页单 Agent / 对话页多 Agent 聚合）
//! - `AgentTracePanel`: Agent 详情页轨迹面板（左侧 run 列表 + 右侧记忆段 / 轮次时间线主从联动）
//! - `MemoryTraceDetail`: 单条记忆轨迹详情（input / output / metadata），面板与弹窗共用
//! - `ThinkRoundsTimeline`: 思考轮次时间线（面板与思考轨迹 Tab 共用）
//! - `ThinkPathGraphView`: 思考路径图视图（同名节点复用 + 转移边聚合 + 图/列表双视图 +
//!   缩略图 + 放大弹窗；思考轨迹 Tab 与 Agent 轨迹面板共用同一组件，防两处漂移）
//! - `ProjectAgentsTab`: 项目模式 Agent Tab —— 项目内 Agent 列表（assignee 推导）
//!   与单 Agent 详情子页（复用 `AgentInfoTab`），面板内视图切换不走路由
//! - `CancelThinkingButton`: 「停止思考」按钮（含二次确认），对话内取消目标 Agent
//!   当前思考轮次；置底状态气泡与输入区发送键位两个落点共用

pub mod agent_trace_panel;
pub mod cancel_thinking_button;
pub mod chat_side_panel;
pub mod memory_trace_detail;
pub mod message_bubble;
pub mod project_agents_tab;
pub mod think_path_graph_view;
pub mod think_rounds_timeline;
pub mod think_traces_tab;
pub mod tool_calls_tab;
pub mod typing_indicator;

pub use agent_trace_panel::AgentTracePanel;
pub use cancel_thinking_button::{CancelThinkingButton, CancelThinkingStyle};
pub use chat_side_panel::ChatSidePanel;
pub use message_bubble::MessageBubble;
pub use project_agents_tab::ProjectAgentsTab;
pub use think_path_graph_view::ThinkPathGraphView;
pub use think_traces_tab::ThinkTracesTab;
pub use tool_calls_tab::ToolCallsTab;
pub use typing_indicator::TypingIndicator;
