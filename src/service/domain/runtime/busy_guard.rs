//! Agent Busy 状态的 RAII guard
//!
//! 确保无论 awaken 返回成功还是失败（包括 ? 提早返回和 panic），
//! Agent 的 Busy 状态都会被清理为 Idle。
//! 修复 set_busy 与 set_idle 之间的状态泄漏 bug。

use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;

/// RAII guard：创建时无需设置 Busy（调用方已设置），drop 时自动 set_idle
///
/// 使用方式：
/// ```ignore
/// AgentRuntimeStateManager::global().set_busy(&agent_id, &message_id);
/// let _guard = BusyGuard::new(agent_id);
/// // ... 后续所有 ? 提早返回都会触发 guard drop → set_idle
/// ```
pub struct BusyGuard {
    agent_id: String,
}

impl BusyGuard {
    /// 创建 guard，drop 时自动调用 set_idle
    pub fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        AgentRuntimeStateManager::global().set_idle(&self.agent_id);
    }
}
