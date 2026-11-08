//! 运行时身份数据集（注入需求 `user_info` / `agent_info` 的取值来源）
//!
//! 凭据域把「注入需求」泛化后有两种源：**凭据**（`user_credentials` + 解析链）
//! 与**运行时身份**（本模块）。身份不是秘密、不落库、不可绑定，取值规则见方案
//! v0.4（artifact `01a1bac6-f618`）：
//!
//! | 字段 | `agent_info` | `user_info` |
//! |---|---|---|
//! | `id` | `agents.id` / `ctx.agent_id` | `users.id` / `ctx.user_id` |
//! | `name` / `username` | `agents.name` | `users.username`（DB 不可用时回退 ctx） |
//! | `display_name` | — | `users.display_name` |
//! | `email` | 派生邮箱（`<role_slug>@agents.ai-orz.local`） | `users.email` |
//! | `git_name` | 派生邮箱的 `@` 前缀（role slug） | — |
//! | `role` / `roles` | `agents.role[0]` / 全量角色 | — |
//! | `tags` | `agents.capabilities`（JSON 数组） | — |
//!
//! 派生结果走**进程级缓存**（key = agent_id / user_id，TTL 300s，含负命中），
//! 避免每次工具调用打库；派生失败返回 `None`，由编排层「跳过该条不阻断」。
//!
//! 与 [`crate::pkg::tool_registry::agent_identity`] 的关系：后者是 L1 结构层
//! （`GIT_AUTHOR_*` 四元组，只读 `role` + 同 role 人数），本模块是 L2 可配层
//! （按 `credential_requirements` 的 `field` 取任意白名单字段）。两者共用同一
//! 派生规则（本模块直接复用 `derive_identity`），缓存各自独立、TTL 一致。

use crate::pkg::tool_registry::agent_identity;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 派生结果缓存 TTL（与 L1 保持一致：Agent 资料变更 5 分钟内生效）
pub const CACHE_TTL: Duration = Duration::from_secs(300);

/// Agent 身份数据集
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDataset {
    /// `agents.id`
    pub id: String,
    /// `agents.name`（中文显示名）
    pub name: String,
    /// 派生邮箱（`<role_slug>@agents.ai-orz.local`，多 Agent 同 role 时带短码）
    pub email: String,
    /// 派生邮箱的 `@` 前缀（= role slug）
    pub git_name: String,
    /// 首个角色 slug（`agents.role[0]`）
    pub role: String,
    /// 全量角色 slug（`agents.role` JSON 数组）
    pub roles: Vec<String>,
    /// `agents.capabilities`（JSON 数组）
    pub tags: Vec<String>,
}

impl AgentDataset {
    /// 按 `agent_info` 白名单字段取值；空值 / 未知字段 → `None`（派生失败）
    pub fn field_value(&self, field: &str) -> Option<String> {
        match field {
            "id" => non_empty(&self.id),
            "name" => non_empty(&self.name),
            "email" => non_empty(&self.email),
            "git_name" => non_empty(&self.git_name),
            "role" => non_empty(&self.role),
            "roles" => join_non_empty(&self.roles),
            "tags" => join_non_empty(&self.tags),
            _ => None,
        }
    }
}

/// 用户身份数据集
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDataset {
    /// `users.id`
    pub id: String,
    /// `users.username`
    pub username: String,
    /// `users.display_name`
    pub display_name: String,
    /// `users.email`
    pub email: String,
}

impl UserDataset {
    /// 按 `user_info` 白名单字段取值；空值 / 未知字段 → `None`
    pub fn field_value(&self, field: &str) -> Option<String> {
        match field {
            "id" => non_empty(&self.id),
            "username" => non_empty(&self.username),
            "display_name" => non_empty(&self.display_name),
            "email" => non_empty(&self.email),
            _ => None,
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn join_non_empty(values: &[String]) -> Option<String> {
    let joined = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    non_empty(&joined)
}

/// 派生并缓存 Agent 身份数据集（miss 时查库，命中/负命中直接返回）
pub async fn agent_dataset(pool: &SqlitePool, agent_id: &str) -> Option<AgentDataset> {
    let agent_id = agent_id.trim();
    if agent_id.is_empty() {
        return None;
    }
    if let Some(cached) = agent_cache_lookup(agent_id) {
        return cached;
    }
    let dataset = query_agent_dataset(pool, agent_id).await;
    agent_cache_store(agent_id, dataset.clone(), Instant::now());
    dataset
}

/// 派生并缓存用户身份数据集
///
/// `ctx_username`：`RequestContext.username` 兜底——`users` 表不可用（如测试上下文
/// 未建表）时仍能派生 `username`，`email` / `display_name` 缺失即视为派生失败。
pub async fn user_dataset(
    pool: &SqlitePool,
    user_id: &str,
    ctx_username: Option<&str>,
) -> Option<UserDataset> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return None;
    }
    if let Some(cached) = user_cache_lookup(user_id) {
        return cached;
    }
    let dataset = match query_user_dataset(pool, user_id).await {
        Some(dataset) => Some(dataset),
        None => ctx_username.and_then(|username| {
            non_empty(username).map(|username| UserDataset {
                id: user_id.to_string(),
                display_name: username.clone(),
                username,
                email: String::new(),
            })
        }),
    };
    user_cache_store(user_id, dataset.clone(), Instant::now());
    dataset
}

/// 一条 SQL 取 `agents` 的身份字段 + 同 role 在用 Agent 数（派生邮箱唯一性用）
async fn query_agent_dataset(pool: &SqlitePool, agent_id: &str) -> Option<AgentDataset> {
    let row = match sqlx::query(
        r#"SELECT a.name AS name,
                  a.role AS role,
                  a.capabilities AS capabilities,
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
            sys_warn!("resolve agent dataset for {agent_id} failed: {error}");
            return None;
        }
    }?;
    let role_json: Option<String> = row.try_get("role").ok();
    let name: Option<String> = row.try_get("name").ok();
    let capabilities: Option<String> = row.try_get("capabilities").ok();
    let peers: i64 = row.try_get("peers").unwrap_or(1);
    let identity = agent_identity::derive_identity(role_json.as_deref(), agent_id, peers)?;
    let roles = role_slugs(role_json.as_deref());
    Some(AgentDataset {
        id: agent_id.to_string(),
        name: name.unwrap_or_else(|| identity.name.clone()),
        git_name: identity.name.clone(),
        email: identity.email,
        role: roles.first().cloned().unwrap_or_default(),
        roles,
        tags: json_string_array(capabilities.as_deref()),
    })
}

async fn query_user_dataset(pool: &SqlitePool, user_id: &str) -> Option<UserDataset> {
    let row = match sqlx::query(
        "SELECT id AS id, username AS username, display_name AS display_name, email AS email \
           FROM users WHERE id = ?1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(error) => {
            sys_warn!("resolve user dataset for {user_id} failed: {error}");
            return None;
        }
    }?;
    Some(UserDataset {
        id: row
            .try_get::<String, _>("id")
            .unwrap_or_else(|_| user_id.to_string()),
        username: row.try_get::<String, _>("username").unwrap_or_default(),
        display_name: row.try_get::<String, _>("display_name").unwrap_or_default(),
        email: row.try_get::<String, _>("email").unwrap_or_default(),
    })
}

/// `agents.role` JSON → 全量角色 slug（容错：数组 / 裸字符串 / 非法 JSON）
fn role_slugs(role_json: Option<&str>) -> Vec<String> {
    let Some(raw) = role_json.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .filter_map(normalize_slug)
            .collect(),
        Ok(serde_json::Value::String(value)) => normalize_slug(&value).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// `agents.capabilities` JSON 数组 → 字符串列表（非数组/非法 JSON → 空）
fn json_string_array(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// slug 归一（与 L1 `agent_identity` 的规则逐字一致：小写 + 非 `[a-z0-9_.]` 折 `-`）
fn normalize_slug(raw: &str) -> Option<String> {
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
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

// ==================== 进程级缓存（TTL 300s，含负命中） ====================

struct Cached<T> {
    at: Instant,
    value: Option<T>,
}

fn agent_cache() -> &'static Mutex<HashMap<String, Cached<AgentDataset>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached<AgentDataset>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn user_cache() -> &'static Mutex<HashMap<String, Cached<UserDataset>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Cached<UserDataset>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn agent_cache_lookup(key: &str) -> Option<Option<AgentDataset>> {
    let guard = agent_cache().lock().ok()?;
    let entry = guard.get(key)?;
    if entry.at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.value.clone())
}

fn agent_cache_store(key: &str, value: Option<AgentDataset>, at: Instant) {
    if let Ok(mut guard) = agent_cache().lock() {
        guard.insert(key.to_string(), Cached { at, value });
    }
}

fn user_cache_lookup(key: &str) -> Option<Option<UserDataset>> {
    let guard = user_cache().lock().ok()?;
    let entry = guard.get(key)?;
    if entry.at.elapsed() > CACHE_TTL {
        return None;
    }
    Some(entry.value.clone())
}

fn user_cache_store(key: &str, value: Option<UserDataset>, at: Instant) {
    if let Ok(mut guard) = user_cache().lock() {
        guard.insert(key.to_string(), Cached { at, value });
    }
}

/// 清空缓存（Agent/用户资料变更需立即生效时调用；测试亦用它隔离用例）
pub fn clear_cache() {
    if let Ok(mut guard) = agent_cache().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = user_cache().lock() {
        guard.clear();
    }
}

#[cfg(test)]
#[path = "identity_dataset_tests.rs"]
mod tests;
