//! Agent 级 git 身份**纯派生规则**（无 DB、无缓存、无 env）
//!
//! 本模块只回答一个问题：给定 `agents.role` JSON + `agent_id` + 同 role 在用人数，
//! 该 Agent 的 `git_name` 与 `email` 应该是什么。**取值链路已统一到凭据注入通道**：
//! `shell_exec` 的出厂默认 `credential_requirements` 声明 `agent_info.git_name` /
//! `agent_info.email` → `Env{GIT_AUTHOR_NAME}` 等四条，值由
//! [`crate::pkg::identity_dataset`] 运行时从 `agents` 表派生（用户可改可删）。
//!
//! 本模块因此**不再持有 SQL、缓存与环境变量常量**：它只是
//! [`crate::pkg::identity_dataset`] 复用的口径单点（`git_name` 与 `email` 必须同源，
//! 否则姓名与邮箱会指向两个不同角色）。
//!
//! # 为什么是 env 而不是仓库配置
//!
//! git 身份优先级：`GIT_AUTHOR_*` / `GIT_COMMITTER_*` env 高于仓库 `.git/config`，
//! 仓库配置高于全局 `~/.gitconfig`。写仓库配置有两个硬伤：一是同仓所有 Agent 共用
//! 一套身份；二是 worktree 共享 `.git/config`，一改全变。env 注入等价且零仓库改动。
//!
//! # 派生规则
//!
//! - `NAME` = `agents.role[0]`（JSON 数组首元素，如 `tech_lead`）
//! - `EMAIL` = `<role_slug>@agents.ai-orz.local`
//! - 同 role 有多个在用 Agent 时 → `<role_slug>+<agent_id 前 8>@agents.ai-orz.local`
//!   （当前各 role 唯一，此分支为将来预留）
//! - 无 role / role 非法 → `None`（**不凭空造值**）
//!
//! `agents` 表没有 email 列（零迁移约束），`agents.ai-orz.local` 是本地占位域：
//! GitHub 侧认不出该邮箱（显示为未关联作者），持久归属仍靠 commit trailer
//! （`Agent-Id` / `Task-Id` / `Project-Id`）与 PR 描述承载。

/// Agent 身份邮箱域名（本地占位域，非真实可投递邮箱）
pub const EMAIL_DOMAIN: &str = "agents.ai-orz.local";

/// Agent 级 git 身份（`git_name` / `email` 两个派生值）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGitIdentity {
    /// `git_name` 的取值（注入 `GIT_AUTHOR_NAME` / `GIT_COMMITTER_NAME`）
    pub name: String,
    /// `email` 的取值（注入 `GIT_AUTHOR_EMAIL` / `GIT_COMMITTER_EMAIL`）
    pub email: String,
}

/// 纯函数：`agents.role` JSON + agent_id + 同 role 在用 Agent 数 → 身份串
///
/// `peers` 是含自己在内的 COUNT 口径，故 `peers > 1` 表示该 role 不唯一。
pub fn derive_identity(
    role_json: Option<&str>,
    agent_id: &str,
    peers: i64,
) -> Option<AgentGitIdentity> {
    let slug = first_role_slug(role_json)?;
    let email = if peers > 1 {
        let short: String = agent_id.trim().chars().take(8).collect();
        format!("{slug}+{short}@{EMAIL_DOMAIN}")
    } else {
        format!("{slug}@{EMAIL_DOMAIN}")
    };
    Some(AgentGitIdentity { name: slug, email })
}

/// `role` JSON → 首个角色 slug（容错：数组 / 裸字符串 / 空白）
fn first_role_slug(role_json: Option<&str>) -> Option<String> {
    let raw = role_json?.trim();
    if raw.is_empty() {
        return None;
    }
    let candidate = match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(items)) => items
            .first()
            .and_then(|item| item.as_str())
            .map(str::to_string),
        Ok(serde_json::Value::String(value)) => Some(value),
        _ => None,
    }?;
    sanitize_slug(&candidate)
}

/// slug 归一：小写 + 非 `[a-z0-9_.]` 字符折成 `-`（连续折叠）+ 去首尾 `-`
///
/// git 的 `user.name` 本身允许空格，但身份串要能原样进 env / 日志 /
/// 邮箱局部部分，ASCII slug 是唯一不需要转义的选择（中文显示名留在 `agents.name`）。
fn sanitize_slug(raw: &str) -> Option<String> {
    let mut slug = String::with_capacity(raw.len());
    let mut last_dash = false;
    for ch in raw.trim().chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() || lowered == '_' || lowered == '.' {
            slug.push(lowered);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
#[path = "agent_identity_tests.rs"]
mod tests;
