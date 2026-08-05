//! Agent 运行时状态管理器
//!
//! 纯内存状态，全局单例，不持久化。
//! 服务重启后状态自动重置（Agent 相当于自动休息）。

use crate::models::events::AgentStateEvent;
use common::enums::AgentRuntimeState;
use dashmap::DashMap;
use std::sync::Arc;

/// Agent 运行时信息（内存中）
#[derive(Debug, Clone)]
pub struct AgentRuntimeInfo {
    pub state: AgentRuntimeState,
    /// 当前处理的消息 ID（仅 Busy 时有效）
    pub current_message_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
}

impl Default for AgentRuntimeInfo {
    fn default() -> Self {
        Self {
            state: AgentRuntimeState::Idle,
            current_message_id: None,
            state_started_at: 0,
        }
    }
}

/// Agent 运行时状态管理器（全局单例）
pub struct AgentRuntimeStateManager {
    states: DashMap<String, AgentRuntimeInfo>,
}

impl AgentRuntimeStateManager {
    /// 创建新的管理器实例（用于测试）
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }

    /// 获取全局单例
    pub fn global() -> Arc<Self> {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<Arc<AgentRuntimeStateManager>> = OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(Self::new())).clone()
    }

    /// 设置 Agent 为空闲状态
    pub fn set_idle(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Idle;
        entry.current_message_id = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry); // 释放 dashmap 借用
        self.notify_state_change(agent_id, state_str(from_state), "idle", None);
    }

    /// 设置 Agent 为休息状态
    pub fn set_resting(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Resting;
        entry.current_message_id = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry);
        self.notify_state_change(agent_id, state_str(from_state), "resting", None);
    }

    /// 设置 Agent 为忙碌状态
    pub fn set_busy(&self, agent_id: &str, message_id: &str) {
        let from_state = self.get_state(agent_id);
        let msg_id = message_id.to_string();
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Busy;
        entry.current_message_id = Some(msg_id.clone());
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry);
        self.notify_state_change(agent_id, state_str(from_state), "busy", Some(msg_id));
    }

    /// 原子地尝试设置 Busy 状态
    ///
    /// 如果 Agent 当前是 Idle，设置为 Busy 并返回 true。
    /// 如果 Agent 当前是 Busy 或 Resting，返回 false（未修改状态）。
    ///
    /// 修复 TOCTOU 竞态：consumer 的 is_unavailable 检查与 awaken 的 set_busy 之间
    /// 会被其他 worker 插入，导致同一 Agent 被并发唤醒。
    pub fn try_set_busy(&self, agent_id: &str, message_id: &str) -> bool {
        let from_state;
        let msg_id = message_id.to_string();
        {
            let mut entry = self.states.entry(agent_id.to_string()).or_default();
            if entry.state.is_unavailable() {
                return false;
            }
            from_state = entry.state;
            entry.state = AgentRuntimeState::Busy;
            entry.current_message_id = Some(msg_id.clone());
            entry.state_started_at = common::constants::utils::current_timestamp_ms();
        }
        self.notify_state_change(agent_id, state_str(from_state), "busy", Some(msg_id));
        true
    }

    /// 获取 Agent 运行时信息
    pub fn get(&self, agent_id: &str) -> Option<AgentRuntimeInfo> {
        self.states.get(agent_id).map(|v| v.clone())
    }

    /// 获取 Agent 运行时状态（不存在则返回 Idle）
    pub fn get_state(&self, agent_id: &str) -> AgentRuntimeState {
        self.get(agent_id)
            .map(|info| info.state)
            .unwrap_or(AgentRuntimeState::Idle)
    }

    /// Agent 是否不可用（忙碌或休息）
    pub fn is_unavailable(&self, agent_id: &str) -> bool {
        self.get_state(agent_id).is_unavailable()
    }

    /// 获取所有 Agent 的运行时状态（用于列表查询）
    pub fn get_all_states(&self) -> Vec<(String, AgentRuntimeInfo)> {
        self.states
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// 发布状态变更事件（AOP 同步转发）
    ///
    /// 同步方法中无法 await，使用 tokio::spawn 异步发布事件。
    /// 事件为 fire-and-forget，不影响业务流程。
    fn notify_state_change(
        &self,
        agent_id: &str,
        from_state: &str,
        to_state: &str,
        message_id: Option<String>,
    ) {
        let agent_id = agent_id.to_string();
        let from_state = from_state.to_string();
        let to_state = to_state.to_string();
        tokio::spawn(async move {
            let _ = crate::pkg::aop::publish(AgentStateEvent::new(
                &agent_id,
                &from_state,
                &to_state,
                message_id,
            ))
            .await;
        });
    }
}

impl Default for AgentRuntimeStateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 将 AgentRuntimeState 转为字符串（事件用）
fn state_str(state: AgentRuntimeState) -> &'static str {
    match state {
        AgentRuntimeState::Idle => "idle",
        AgentRuntimeState::Busy => "busy",
        AgentRuntimeState::Resting => "resting",
    }
}
