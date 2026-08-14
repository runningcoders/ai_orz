//! Agent 运行时状态管理器
//!
//! 纯内存状态，全局单例，不持久化。
//! 服务重启后状态自动重置（Agent 相当于自动休息）。

use crate::models::events::AgentStateEvent;
use common::enums::AgentRuntimeState;
use common::enums::ThinkingScene;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

/// Agent 运行时信息（内存中）
#[derive(Debug, Clone)]
pub struct AgentRuntimeInfo {
    pub state: AgentRuntimeState,
    /// 当前处理的消息 ID（仅 Busy 时有效）
    pub current_message_id: Option<String>,
    /// 状态开始时间戳（毫秒）
    pub state_started_at: i64,
    /// 当前关联的任务 ID（set_busy 时设置，set_idle 时清空，set_resting 时保留）
    pub task_id: Option<String>,
    /// 当前关联的项目 ID（同上）
    pub project_id: Option<String>,
    /// 思考运行时（仅 Busy 时有值）
    pub think_runtime: Option<Arc<AgentThinkRuntime>>,
}

impl Default for AgentRuntimeInfo {
    fn default() -> Self {
        Self {
            state: AgentRuntimeState::Idle,
            current_message_id: None,
            state_started_at: 0,
            task_id: None,
            project_id: None,
            think_runtime: None,
        }
    }
}

/// 思考状态
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThinkStatus {
    #[default]
    Thinking,
    Cancelled,
    Finished,
}

/// 思考运行时快照（前端查询用，原子读写）
#[derive(Debug, Clone, Default)]
pub struct ThinkRuntimeSnapshot {
    pub agent_id: String,
    pub trace_id: String,
    /// 场景（ThinkingScene 枚举，定义在 common 层，无循环依赖）
    pub scene: ThinkingScene,
    pub round: usize,
    pub max_rounds: usize,
    pub tokens_input: u64,
    pub tokens_output: u64,
    pub total_tokens: u64,
    pub tool_call_count: usize,
    pub status: ThinkStatus,
    pub started_at: i64,
    pub last_updated_at: i64,
}

impl ThinkRuntimeSnapshot {
    pub fn new(agent_id: String, trace_id: String) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            agent_id,
            trace_id,
            started_at: now,
            last_updated_at: now,
            ..Default::default()
        }
    }

    /// think_loop 每轮上报时更新快照
    #[allow(clippy::too_many_arguments)]
    pub fn report_round(
        &mut self,
        trace_id: &str,
        scene: ThinkingScene,
        round: usize,
        max_rounds: usize,
        tokens_input: u64,
        tokens_output: u64,
        total_tokens: u64,
        tool_call_count: usize,
    ) {
        self.trace_id = trace_id.to_string();
        self.scene = scene;
        self.round = round;
        self.max_rounds = max_rounds;
        self.tokens_input = tokens_input;
        self.tokens_output = tokens_output;
        self.total_tokens = total_tokens;
        self.tool_call_count = tool_call_count;
        self.last_updated_at = common::constants::utils::current_timestamp_ms();
    }
}

/// Agent 思考运行时：跟着 Agent 状态走，Busy 时存在，Idle 时清理
/// 持有 cancel 信号 + 运行时快照，由 think_loop 每轮上报
pub struct AgentThinkRuntime {
    agent_id: String,
    cancel_flag: Arc<AtomicBool>,
    snapshot: RwLock<ThinkRuntimeSnapshot>,
}

impl std::fmt::Debug for AgentThinkRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let snap = self.snapshot.read().map(|s| s.clone()).unwrap_or_default();
        f.debug_struct("AgentThinkRuntime")
            .field("agent_id", &self.agent_id)
            .field("cancelled", &self.cancel_flag.load(Ordering::Relaxed))
            .field("snapshot", &snap)
            .finish()
    }
}

impl AgentThinkRuntime {
    pub fn new(agent_id: String, trace_id: String) -> Self {
        Self {
            agent_id: agent_id.clone(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            snapshot: RwLock::new(ThinkRuntimeSnapshot::new(agent_id, trace_id)),
        }
    }

    /// 获取 agent_id
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// 获取 cancel_flag 的 Arc 引用（用于构造 UserCancelPolicy）
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel_flag.clone()
    }

    /// think_loop 每轮上报时调用
    #[allow(clippy::too_many_arguments)]
    pub fn report_round(
        &self,
        trace_id: &str,
        scene: ThinkingScene,
        round: usize,
        max_rounds: usize,
        tokens_input: u64,
        tokens_output: u64,
        total_tokens: u64,
        tool_call_count: usize,
    ) {
        if let Ok(mut snap) = self.snapshot.write() {
            snap.report_round(
                trace_id,
                scene,
                round,
                max_rounds,
                tokens_input,
                tokens_output,
                total_tokens,
                tool_call_count,
            );
        }
    }

    /// 标记为完成（think_loop 正常结束时调用）
    pub fn finish(&self) {
        if let Ok(mut snap) = self.snapshot.write() {
            snap.status = ThinkStatus::Finished;
        }
    }

    /// 用户取消（由 StateManager.cancel_thinking 调用）
    pub fn cancel(&self) -> bool {
        self.cancel_flag.store(true, Ordering::Relaxed);
        if let Ok(mut snap) = self.snapshot.write() {
            snap.status = ThinkStatus::Cancelled;
        }
        true
    }

    /// 是否已取消（think_loop 每轮检查）
    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::Relaxed)
    }

    /// 获取运行时快照（前端查询用）
    pub fn snapshot(&self) -> ThinkRuntimeSnapshot {
        self.snapshot
            .read()
            .map(|s| s.clone())
            .unwrap_or_default()
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
        entry.task_id = None;
        entry.project_id = None;
        entry.think_runtime = None;
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry); // 释放 dashmap 借用
        self.notify_state_change(agent_id, state_str(from_state), "idle", None);
    }

    /// 挂载思考运行时（consumer 创建后调用）
    pub fn set_think_runtime(&self, agent_id: &str, think_runtime: Arc<AgentThinkRuntime>) {
        if let Some(mut entry) = self.states.get_mut(agent_id) {
            entry.think_runtime = Some(think_runtime);
        }
    }

    /// 清理思考运行时（BusyGuard Drop 时调用）
    pub fn clear_think_runtime(&self, agent_id: &str) {
        if let Some(mut entry) = self.states.get_mut(agent_id) {
            entry.think_runtime = None;
        }
    }

    /// 取消思考（cancel-thinking 接口调用）
    ///
    /// 返回 true 表示成功取消（Agent 正在思考），
    /// 返回 false 表示 Agent 当前未在思考或运行时已清理。
    pub fn cancel_thinking(&self, agent_id: &str) -> bool {
        if let Some(entry) = self.states.get(agent_id)
            && let Some(ref think_runtime) = entry.think_runtime
        {
            return think_runtime.cancel();
        }
        false
    }

    /// 查询思考运行时快照（runtime-status 接口调用）
    pub fn get_think_runtime_snapshot(&self, agent_id: &str) -> Option<ThinkRuntimeSnapshot> {
        self.states.get(agent_id).and_then(|entry| {
            entry.think_runtime.as_ref().map(|tr| tr.snapshot())
        })
    }

    /// 设置 Agent 为休息状态
    pub fn set_resting(&self, agent_id: &str) {
        let from_state = self.get_state(agent_id);
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Resting;
        entry.current_message_id = None;
        // 注意：task_id / project_id 保留不清空
        // 沉淀（sleep_and_settle）在 awaken 的 Busy 期间触发，仍在同一业务上下文中
        entry.state_started_at = common::constants::utils::current_timestamp_ms();
        drop(entry);
        self.notify_state_change(agent_id, state_str(from_state), "resting", None);
    }

    /// 设置 Agent 为忙碌状态
    ///
    /// task_id / project_id 为业务上下文，整个 Busy 期间不变，
    /// 用于前端按任务/项目视角过滤运行中 Agent。
    pub fn set_busy(
        &self,
        agent_id: &str,
        message_id: &str,
        task_id: Option<&str>,
        project_id: Option<&str>,
    ) {
        let from_state = self.get_state(agent_id);
        let msg_id = message_id.to_string();
        let mut entry = self.states.entry(agent_id.to_string()).or_default();
        entry.state = AgentRuntimeState::Busy;
        entry.current_message_id = Some(msg_id.clone());
        entry.task_id = task_id.map(|s| s.to_string());
        entry.project_id = project_id.map(|s| s.to_string());
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
    pub fn try_set_busy(
        &self,
        agent_id: &str,
        message_id: &str,
        task_id: Option<&str>,
        project_id: Option<&str>,
    ) -> bool {
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
            entry.task_id = task_id.map(|s| s.to_string());
            entry.project_id = project_id.map(|s| s.to_string());
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

    /// 查询运行中 Agent 列表（带过滤参数）
    ///
    /// 过滤参数均为 Option，None 表示不过滤。
    /// state_filter: "busy" / "resting" / "idle"（None 返回全部）
    pub fn list_runtime_agents(
        &self,
        state_filter: Option<&str>,
        task_id_filter: Option<&str>,
        project_id_filter: Option<&str>,
    ) -> Vec<(String, AgentRuntimeInfo)> {
        self.states
            .iter()
            .filter(|entry| {
                let info = entry.value();
                // 状态过滤
                if let Some(state) = state_filter {
                    let info_state = match info.state {
                        AgentRuntimeState::Idle => "idle",
                        AgentRuntimeState::Busy => "busy",
                        AgentRuntimeState::Resting => "resting",
                    };
                    if info_state != state {
                        return false;
                    }
                }
                // 任务 ID 过滤
                if let Some(tid) = task_id_filter
                    && info.task_id.as_deref() != Some(tid)
                {
                    return false;
                }
                // 项目 ID 过滤
                if let Some(pid) = project_id_filter
                    && info.project_id.as_deref() != Some(pid)
                {
                    return false;
                }
                true
            })
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// 发布状态变更事件（AOP 同步转发）
    ///
    /// 同步方法中无法 await，使用 tokio::spawn 异步发布事件。
    /// 事件为 fire-and-forget，不影响业务流程。
    /// 无 Tokio runtime 上下文时（如单元测试）跳过事件发布。
    fn notify_state_change(
        &self,
        agent_id: &str,
        from_state: &str,
        to_state: &str,
        message_id: Option<String>,
    ) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_busy_records_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Busy);
        assert_eq!(info.current_message_id, Some("msg-1".to_string()));
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }

    #[test]
    fn test_set_busy_with_none_context() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.task_id, None);
        assert_eq!(info.project_id, None);
    }

    #[test]
    fn test_try_set_busy_records_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        let acquired = mgr.try_set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        assert!(acquired);
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }

    #[test]
    fn test_set_idle_clears_context() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_idle("agent-1");
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Idle);
        assert_eq!(info.current_message_id, None);
        assert_eq!(info.task_id, None);
        assert_eq!(info.project_id, None);
    }

    #[test]
    fn test_set_resting_preserves_task_and_project() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        mgr.set_resting("agent-1");
        let info = mgr.get("agent-1").unwrap();
        assert_eq!(info.state, AgentRuntimeState::Resting);
        // 沉淀场景：清空 message_id，但保留 task_id / project_id（同一业务上下文）
        assert_eq!(info.current_message_id, None);
        assert_eq!(info.task_id, Some("task-1".to_string()));
        assert_eq!(info.project_id, Some("proj-1".to_string()));
    }

    #[test]
    fn test_set_think_runtime_attaches_to_busy_agent() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
        mgr.set_think_runtime("agent-1", tr.clone());

        let info = mgr.get("agent-1").unwrap();
        assert!(info.think_runtime.is_some());
        assert_eq!(info.think_runtime.as_ref().unwrap().agent_id(), "agent-1");

        let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
        assert_eq!(snap.trace_id, "trace-1");
    }

    #[test]
    fn test_set_idle_clears_think_runtime() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
        mgr.set_think_runtime("agent-1", tr);
        mgr.set_idle("agent-1");

        let info = mgr.get("agent-1").unwrap();
        assert!(info.think_runtime.is_none());
        assert!(mgr.get_think_runtime_snapshot("agent-1").is_none());
    }

    #[test]
    fn test_cancel_thinking_signals_cancel() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
        let flag = tr.cancel_flag();
        mgr.set_think_runtime("agent-1", tr);

        assert_ne!(
            mgr.get_think_runtime_snapshot("agent-1").unwrap().status,
            ThinkStatus::Cancelled
        );
        assert!(mgr.cancel_thinking("agent-1"));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));

        let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
        assert_eq!(snap.status, ThinkStatus::Cancelled);
    }

    #[test]
    fn test_cancel_thinking_returns_false_when_not_thinking() {
        let mgr = AgentRuntimeStateManager::new();
        // Idle agent
        assert!(!mgr.cancel_thinking("agent-1"));

        // Busy but no think_runtime attached
        mgr.set_busy("agent-1", "msg-1", None, None);
        assert!(!mgr.cancel_thinking("agent-1"));
    }

    #[test]
    fn test_report_round_updates_snapshot() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
        mgr.set_think_runtime("agent-1", tr.clone());

        tr.report_round(
            "trace-1",
            ThinkingScene::Awaken,
            3,
            365,
            1000,
            500,
            1500,
            2,
        );
        let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
        assert_eq!(snap.round, 3);
        assert_eq!(snap.max_rounds, 365);
        assert_eq!(snap.tokens_input, 1000);
        assert_eq!(snap.tokens_output, 500);
        assert_eq!(snap.total_tokens, 1500);
        assert_eq!(snap.tool_call_count, 2);
        assert_eq!(snap.scene, ThinkingScene::Awaken);
    }

    #[test]
    fn test_clear_think_runtime_explicit() {
        let mgr = AgentRuntimeStateManager::new();
        mgr.set_busy("agent-1", "msg-1", None, None);
        let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
        mgr.set_think_runtime("agent-1", tr);
        assert!(mgr.get_think_runtime_snapshot("agent-1").is_some());

        mgr.clear_think_runtime("agent-1");
        assert!(mgr.get_think_runtime_snapshot("agent-1").is_none());
    }

    /// 构造测试数据：4 个 Agent 覆盖 busy / resting / idle 与不同 task/project 组合
    fn setup_list_runtime_agents(mgr: &AgentRuntimeStateManager) {
        // agent-1: busy, task-1, proj-1
        mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
        // agent-2: busy, task-2, proj-1
        mgr.set_busy("agent-2", "msg-2", Some("task-2"), Some("proj-1"));
        // agent-3: resting, task-1, proj-2（沉淀保留 task/project）
        mgr.set_busy("agent-3", "msg-3", Some("task-1"), Some("proj-2"));
        mgr.set_resting("agent-3");
        // agent-4: idle（无上下文）
        mgr.set_busy("agent-4", "msg-4", None, None);
        mgr.set_idle("agent-4");
    }

    #[test]
    fn test_list_runtime_agents_returns_all_when_no_filter() {
        let mgr = AgentRuntimeStateManager::new();
        setup_list_runtime_agents(&mgr);

        let agents = mgr.list_runtime_agents(None, None, None);
        assert_eq!(agents.len(), 4);
        let ids: Vec<&str> = agents.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"agent-1"));
        assert!(ids.contains(&"agent-2"));
        assert!(ids.contains(&"agent-3"));
        assert!(ids.contains(&"agent-4"));
    }

    #[test]
    fn test_list_runtime_agents_filters_by_state() {
        let mgr = AgentRuntimeStateManager::new();
        setup_list_runtime_agents(&mgr);

        // busy：agent-1, agent-2
        let busy = mgr.list_runtime_agents(Some("busy"), None, None);
        assert_eq!(busy.len(), 2);

        // resting：agent-3
        let resting = mgr.list_runtime_agents(Some("resting"), None, None);
        assert_eq!(resting.len(), 1);
        assert_eq!(resting[0].0, "agent-3");

        // idle：agent-4
        let idle = mgr.list_runtime_agents(Some("idle"), None, None);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].0, "agent-4");

        // 不存在的 state：空结果
        let none = mgr.list_runtime_agents(Some("unknown"), None, None);
        assert!(none.is_empty());
    }

    #[test]
    fn test_list_runtime_agents_filters_by_task_and_project_and_combination() {
        let mgr = AgentRuntimeStateManager::new();
        setup_list_runtime_agents(&mgr);

        // task-1：agent-1, agent-3
        let task1 = mgr.list_runtime_agents(None, Some("task-1"), None);
        assert_eq!(task1.len(), 2);

        // proj-1：agent-1, agent-2
        let proj1 = mgr.list_runtime_agents(None, None, Some("proj-1"));
        assert_eq!(proj1.len(), 2);

        // task-1 + proj-1：仅 agent-1
        let both = mgr.list_runtime_agents(None, Some("task-1"), Some("proj-1"));
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].0, "agent-1");

        // busy + task-1：仅 agent-1（agent-3 是 resting 被排除）
        let combined = mgr.list_runtime_agents(Some("busy"), Some("task-1"), None);
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].0, "agent-1");

        // busy + proj-1：agent-1, agent-2
        let busy_proj1 = mgr.list_runtime_agents(Some("busy"), None, Some("proj-1"));
        assert_eq!(busy_proj1.len(), 2);

        // 不存在的 task：空结果
        let empty = mgr.list_runtime_agents(None, Some("not-exist"), None);
        assert!(empty.is_empty());
    }
}
