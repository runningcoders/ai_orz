//! Agent 级本地 git 身份派生（结构层 L1）
//!
//! `shell_env::resolve` 是本平台唯一的子进程环境出口，本模块为它提供
//! `GIT_AUTHOR_*` / `GIT_COMMITTER_*` 四元组的取值：身份由
//! `RequestContext.agent_id` 从 `agents` 表派生，**不触碰任何仓库 `.git/config`**。
//!
//! # 为什么用 env 而不是仓库配置
//!
//! git 身份优先级：`GIT_AUTHOR_*` / `GIT_COMMITTER_*` env 高于仓库 `.git/config`，
//! 仓库配置高于全局 `~/.gitconfig`。写仓库配置有两个硬伤：一是同仓所有 Agent 共用一套身份
//! （现状正是如此，ai_orz 仓 `.git/config` 写死 `ai-orz agent`）；二是 worktree
//! 共享 `.git/config`，一改全变。env 注入等价且零仓库改动。
//!
//! # 派生规则
//!
//! - `NAME` = `agents.role[0]`（JSON 数组首元素，如 `tech_lead`）
//! - `EMAIL` = `<role_slug>@agents.ai-orz.local`
//! - 同 role 有多个在用 Agent 时 → `<role_slug>+<agent_id 前 8>@agents.ai-orz.local`
//!   （当前各 role 唯一，此分支为将来预留）
//! - 无 agent 上下文 / agent 不存在 / role 缺省 → `None`（**不凭空造值**）
//!
//! `agents` 表没有 email 列（零迁移约束），`agents.ai-orz.local` 是本地占位域：
//! GitHub 侧认不出该邮箱（显示为未关联作者），持久归属仍靠 commit trailer
//! （`Agent-Id` / `Task-Id`）与 PR 描述承载。

use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// `GIT_AUTHOR_NAME`
pub const ENV_GIT_AUTHOR_NAME: &str = "GIT_AUTHOR_NAME";
/// `GIT_AUTHOR_EMAIL`
pub const ENV_GIT_AUTHOR_EMAIL: &str = "GIT_AUTHOR_EMAIL";
/// `GIT_COMMITTER_NAME`
pub const ENV_GIT_COMMITTER_NAME: &str = "GIT_COMMITTER_NAME";
/// `GIT_COMMITTER_EMAIL`
pub const ENV_GIT_COMMITTER_EMAIL: &str = "GIT_COMMITTER_EMAIL";

/// Agent 身份邮箱域名（本地占位域，非真实可投递邮箱）
pub const EMAIL_DOMAIN: &str = "agents.ai-orz.local";

/// 派生结果缓存 TTL：避免每次工具调用打库，同时保证 Agent 改 role 后 5 分钟内生效
pub const CACHE_TTL: Duration = Duration::from_secs(300);

/// Agent 级 git 身份（四元组的取值来源）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGitIdentity {
    /// `GIT_AUTHOR_NAME` / `GIT_COMMITTER_NAME` 的取值
    pub name: String,
    /// `GIT_AUTHOR_EMAIL` / `GIT_COMMITTER_EMAIL` 的取值
    pub email: String,
}

impl AgentGitIdentity {
    /// 待注入的四元组；调用方以 `or_insert` 施加（params.env / L2 优先）
    pub fn env_pairs(&self) -> [(&'static str, &str); 4] {
        [
            (ENV_GIT_AUTHOR_NAME, self.name.as_str()),
            (ENV_GIT_AUTHOR_EMAIL, self.email.as_str()),
            (ENV_GIT_COMMITTER_NAME, self.name.as_str()),
            (ENV_GIT_COMMITTER_EMAIL, self.email.as_str()),
        ]
    }
}

/// 派生并缓存 Agent 级 git 身份（miss 时查库，命中/负命中直接返回）
///
/// 返回 `None` 表示「不注入」：无 agent 上下文由调用方保证（不调用本函数）、
/// agent 不存在或 role 缺省由本函数保证——两种情况都必须零新增变量。
pub async fn resolve(pool: &SqlitePool, agent_id: &str) -> Option<AgentGitIdentity> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return None;
    }
    if let Some(cached) = cache_lookup(agent_id) {
        return cached;
    }
    let identity = query_identity(pool, agent_id).await;
    cache_store(agent_id, identity.clone(), Instant::now());
    identity
}

/// 查库派生：一条 SQL 同时取 role 与「同 role 在用 Agent 数」
///
/// 用运行时 API（非 `sqlx::query!` 宏）：本模块不引入新的编译期 SQL 校验，
/// 免去 `.sqlx` 离线缓存重刷（SQLX_OFFLINE 环境下宏会直接编译失败）。
async fn query_identity(pool: &SqlitePool, agent_id: &str) -> Option<AgentGitIdentity> {
    let row = match sqlx::query(
        r#"SELECT a.role AS role,
                  (SELECT COUNT(*) FROM agents b
                    WHERE b.status <> 0 AND b.role = a.role) AS peers
             FROM agents a
            WHERE a.id = ?1 AND a.status <> 0"#,
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(error) => {
            // best-effort：派生失败不阻断工具执行（退化为仓库既有身份）
            sys_warn!("resolve agent git identity for {agent_id} failed: {error}");
            return None;
        }
    }?;
    let role: Option<String> = row.try_get("role").ok();
    let peers: i64 = row.try_get("peers").unwrap_or(1);
    derive_identity(role.as_deref(), agent_id, peers)
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

/// 进程级缓存条目（`at` = 写入时刻，用于 TTL 判定）
struct CachedIdentity {
    at: Instant,
    identity: Option<AgentGitIdentity>,
}

fn cache() -> &'static Mutex<HashMap<String, CachedIdentity>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedIdentity>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 命中返回 `Some(identity_option)`；未命中 / 已过期 / 锁中毒返回 `None`
fn cache_lookup(agent_id: &str) -> Option<Option<AgentGitIdentity>> {
    let guard = cache().lock().ok()?;
    let entry = guard.get(agent_id)?;
    if entry.at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.identity.clone())
}

fn cache_store(agent_id: &str, identity: Option<AgentGitIdentity>, at: Instant) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(agent_id.to_string(), CachedIdentity { at, identity });
    }
}

/// 清空缓存（Agent 资料变更需立即生效时调用；测试亦用它隔离用例）
pub fn clear_cache() {
    if let Ok(mut guard) = cache().lock() {
        guard.clear();
    }
}

#[cfg(test)]
#[path = "agent_identity_tests.rs"]
mod tests;
