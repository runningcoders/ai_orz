//! 用户维度数据路径约定（base_data_path 下的统一布局）

use std::path::{Path, PathBuf};

/// 用户维度集成的统一 HOME 目录（`{base_data_path}/users/{user_id}`）
///
/// # 约定
///
/// 所有"以用户身份"运行的外部集成（lark-cli、gh 等），HOME 一律注入
/// 本函数返回的目录，禁止按应用维度自建路径（如 `integrations/{app}/{user_id}`）：
///
/// - 各 CLI 按自身默认规则在 HOME 下写配置（`.lark-cli/`、`.config/gh/` 等，XDG 天然不冲突）
/// - `.ssh/`、`.gitconfig` 等跨工具共享配置在同一 HOME 下自然复用
/// - 清理单个集成只删其子目录（如 `~/.config/gh/`），不影响其他集成
///
/// 非用户维度的共享数据（vectors/、seeds/ 等）仍留在 base_data_path 顶层。
pub fn user_home(base_data_path: &Path, user_id: &str) -> PathBuf {
    base_data_path.join("users").join(user_id)
}

/// 用户级共享工作区（`{user_home}/shared`）
///
/// # 落盘约定
///
/// 跨 Agent 协作产物、任务/项目挂靠的长期工作成果放这里：
///
/// - 任务/项目绑定的开发工作 → `shared/projects/{project_id}/`（多 Agent 接力/并行协作的仓库主副本）
/// - 跨 Agent 交付物、临时暂存区 → `shared/` 下按需建子目录
///
/// Agent 默认不在共享区直接落盘：无用户指示时写自己的
/// [`user_agent_workspace`]；用户明确要求或产物为跨 Agent 协作件时才进共享区。
pub fn user_shared_workspace(base_data_path: &Path, user_id: &str) -> PathBuf {
    user_home(base_data_path, user_id).join("shared")
}

/// Agent 为某用户工作时的工作区（`{user_home}/agents/{agent_id}/work`）
///
/// # 语义
///
/// Agent **在该用户上下文中**执行任务（对话、任务、项目）时的默认落盘处：
/// 产物归属该用户（用户委托 Agent 生成），随用户目录整体备份/清理。
///
/// - 挂靠任务/项目的工作仍应优先落到 [`user_shared_workspace`] 的项目目录
/// - 无挂靠的临时请求、Agent 工作副本（如 git worktree）落这里
///
/// 注意与 [`agent_workspace`]（Agent 自身工作区，无用户上下文）区分：
/// 带 `user_` 前缀的函数均在用户目录树下，参数含 `user_id`。
pub fn user_agent_workspace(base_data_path: &Path, user_id: &str, agent_id: &str) -> PathBuf {
    user_home(base_data_path, user_id)
        .join("agents")
        .join(agent_id)
        .join("work")
}

/// Agent 自身工作区（`{base_data_path}/agents/{agent_id}/work`）
///
/// # 语义
///
/// Agent **无用户上下文**的自主行为（定时任务、休息沉淀、后台自我整理等）
/// 的默认落盘处；与 `agents/{agent_id}/` 下的 memory/、skills/ 同级。
///
/// 为用户执行任务的产物不要落这里（应落 [`user_agent_workspace`]），
/// 否则用户备份/清理时无法整体带走。
pub fn agent_workspace(base_data_path: &Path, agent_id: &str) -> PathBuf {
    base_data_path.join("agents").join(agent_id).join("work")
}

/// 按调用身份选择工具默认工作目录
///
/// # 规则
///
/// - 用户 + Agent 上下文（Agent 为用户执行任务）→ [`user_agent_workspace`]
/// - 仅 Agent 上下文（Agent 自主行为，如定时任务）→ [`agent_workspace`]
/// - 无 Agent 上下文（用户直接调用 / 系统调用）→ `base_data_path`
pub fn default_workspace(
    base_data_path: &Path,
    user_id: Option<&str>,
    agent_id: Option<&str>,
) -> PathBuf {
    match (user_id, agent_id) {
        (Some(uid), Some(aid)) => user_agent_workspace(base_data_path, uid, aid),
        (None, Some(aid)) => agent_workspace(base_data_path, aid),
        _ => base_data_path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_home_is_users_rooted() {
        assert_eq!(
            user_home(Path::new("/data/.ai_orz"), "user-001"),
            PathBuf::from("/data/.ai_orz/users/user-001")
        );
    }

    #[test]
    fn user_shared_workspace_under_user_home() {
        assert_eq!(
            user_shared_workspace(Path::new("/data/.ai_orz"), "user-001"),
            PathBuf::from("/data/.ai_orz/users/user-001/shared")
        );
    }

    #[test]
    fn user_agent_workspace_nests_user_and_agent() {
        assert_eq!(
            user_agent_workspace(Path::new("/data/.ai_orz"), "user-001", "agent-007"),
            PathBuf::from("/data/.ai_orz/users/user-001/agents/agent-007/work")
        );
    }

    #[test]
    fn agent_workspace_is_agent_rooted() {
        assert_eq!(
            agent_workspace(Path::new("/data/.ai_orz"), "agent-007"),
            PathBuf::from("/data/.ai_orz/agents/agent-007/work")
        );
    }

    #[test]
    fn default_workspace_follows_caller_identity() {
        let base = Path::new("/data/.ai_orz");
        // 用户 + Agent：为该用户工作
        assert_eq!(
            default_workspace(base, Some("u1"), Some("a1")),
            PathBuf::from("/data/.ai_orz/users/u1/agents/a1/work")
        );
        // 仅 Agent：自主行为
        assert_eq!(
            default_workspace(base, None, Some("a1")),
            PathBuf::from("/data/.ai_orz/agents/a1/work")
        );
        // 无 Agent：用户/系统直接调用回退 base 根
        assert_eq!(
            default_workspace(base, Some("u1"), None),
            PathBuf::from("/data/.ai_orz")
        );
        assert_eq!(
            default_workspace(base, None, None),
            PathBuf::from("/data/.ai_orz")
        );
    }
}
