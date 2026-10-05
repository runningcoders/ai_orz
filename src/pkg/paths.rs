//! 物理路径 SSOT（base_data_path 下的统一布局）
//!
//! 【定位】后端唯一的路径布局约定模块。所有跨文件系统的路径拼接，
//! 只要属于"base_data_path 下的某个固定子目录结构"，都必须通过这里的纯函数获取，
//! 禁止在业务代码中手写 `base.join("attachments")`、`join("agents").join(aid)` 等散串。
//!
//! # 分两大类
//!
//! - **用户维度路径**：`user_` 前缀，参数带 `user_id`，产物都在 `users/{uid}/` 下
//! - **系统/Agent 共享路径**：无前缀，参数只有 `agent_id` 或无子参数，产物在 base 顶层
//!
//! # 签名约定（纯函数）
//!
//! 所有函数第一个参数统一为 `base_data_path: &Path`（从 config 读取后传入），
//! 不隐式依赖全局 `config::get()`，保持可独立测试性。

use std::path::{Path, PathBuf};

// ====================================================================
// 用户维度路径（全部在 users/{uid}/ 下）
// ====================================================================

pub fn user_home(base_data_path: &Path, user_id: &str) -> PathBuf {
    base_data_path.join("users").join(user_id)
}

pub fn user_shared_workspace(base_data_path: &Path, user_id: &str) -> PathBuf {
    user_home(base_data_path, user_id).join("shared")
}

pub fn user_project_root(base_data_path: &Path, user_id: &str, project_id: &str) -> PathBuf {
    user_shared_workspace(base_data_path, user_id)
        .join("projects")
        .join(project_id)
}

pub fn user_project_workspace(base_data_path: &Path, user_id: &str, project_id: &str) -> PathBuf {
    user_project_root(base_data_path, user_id, project_id).join("workspace")
}

pub fn user_agent_workspace(base_data_path: &Path, user_id: &str, agent_id: &str) -> PathBuf {
    user_home(base_data_path, user_id)
        .join("agents")
        .join(agent_id)
        .join("work")
}

// ====================================================================
// 系统 / Agent 共享路径（在 base_data_path 顶层各子目录下）
// ====================================================================

pub fn users_root_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("users")
}

pub fn agent_data_dir(base_data_path: &Path, agent_id: &str) -> PathBuf {
    base_data_path.join("agents").join(agent_id)
}

pub fn agent_workspace(base_data_path: &Path, agent_id: &str) -> PathBuf {
    agent_data_dir(base_data_path, agent_id).join("work")
}

pub fn agent_memory_dir(base_data_path: &Path, agent_id: &str) -> PathBuf {
    agent_data_dir(base_data_path, agent_id).join("memory")
}

pub fn attachments_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("attachments")
}

pub fn artifacts_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("artifacts")
}

pub fn artifact_project_dir(base_data_path: &Path, project_id: &str) -> PathBuf {
    artifacts_dir(base_data_path)
        .join("projects")
        .join(project_id)
}

pub fn artifact_path(base_data_path: &Path, project_id: &str, artifact_id: &str) -> PathBuf {
    artifact_project_dir(base_data_path, project_id).join(artifact_id)
}

// --- 向量存储目录（InMemoryVectorDB / Qdrant 落盘） ---
pub fn vectors_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("vectors")
}

// --- SQLite / VSS / LanceDB / DuckDB / HNSW 具体存储路径 ---
/// SQLite 主数据库文件路径。文件名来自配置 yaml 的 `database.db_file_name`。
pub fn sqlite_db_path(base_data_path: &Path, db_file_name: &str) -> PathBuf {
    base_data_path.join(db_file_name)
}

/// SQLite VSS 扩展向量数据库文件路径。文件名来自配置 `database.vector_db_file_name`。
pub fn vector_sqlite_db_path(base_data_path: &Path, vector_db_file_name: &str) -> PathBuf {
    base_data_path.join(vector_db_file_name)
}

/// LanceDB 高性能嵌入式向量库持久化目录：`{base}/vectors_lance`
pub fn lance_vector_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("vectors_lance")
}

/// HNSW 索引持久化目录。子目录名来自配置 `database.hnsw_index_dir`。
pub fn hnsw_index_dir(base_data_path: &Path, hnsw_dir_name: &str) -> PathBuf {
    base_data_path.join(hnsw_dir_name)
}

/// DuckDB 统计事件数据库文件路径。文件名来自配置 `stats.db_file_name`。
pub fn stats_db_path(base_data_path: &Path, stats_db_file_name: &str) -> PathBuf {
    base_data_path.join(stats_db_file_name)
}

// --- 工具相关目录 ---
/// `{base}/tools` — 工具级产物（调用轨迹、日志）的根目录
pub fn tools_root_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("tools")
}

/// `{base}/tools/call_trace` — 每次工具调用的 trace 文件目录（按天分片）
///
/// 【边界决策 2026-09-19】**不按 `tool_id` 再分一层目录**：
/// - `call_id` 是一次工具调用的唯一身份（UUID v7），`tool_id` 只是 entry 里的字段；
/// - 按 tool_id 分目录会让「未带 tool_id 的查询」（详情页按 call_id 查、「最近 N 条」列表）
///   退化成全量目录枚举 —— 工具越多越慢；
/// - 地址依赖业务字段后，工具改名/迁移会让历史 trace 变成孤儿目录；
/// - 顺带会诱导出「按 (`tool_id`, `call_id`) 收窄」的幂等判定，而幂等只需 `call_id`。
///
/// 拉平后 `tool_id` 仍作为 `ToolCallEntry` 字段参与查询过滤，语义不变。
pub fn tool_call_trace_dir(base_data_path: &Path) -> PathBuf {
    tools_root_dir(base_data_path).join("call_trace")
}

/// `{base}/tools/{tool_id}/logs` — 工具执行日志目录
pub fn tool_logs_dir(base_data_path: &Path, tool_id: &str) -> PathBuf {
    tools_root_dir(base_data_path).join(tool_id).join("logs")
}

// --- 技能目录（内置技能注册 / 落盘） ---
pub fn skills_root_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("skills")
}

/// 共享技能（跨 Agent）目录：`{base}/skills/shared/{skill_id}`
pub fn shared_skill_dir(base_data_path: &Path, skill_id: &str) -> PathBuf {
    skills_root_dir(base_data_path)
        .join("shared")
        .join(skill_id)
}

/// Agent 专属技能目录：`{base}/agents/{agent_id}/skills/{skill_id}`
pub fn agent_skill_dir(base_data_path: &Path, agent_id: &str, skill_id: &str) -> PathBuf {
    agent_data_dir(base_data_path, agent_id)
        .join("skills")
        .join(skill_id)
}

// --- 种子目录（系统推荐 / RAG 种子文件） ---
pub fn seeds_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("seeds")
}

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
#[path = "paths_tests.rs"]
mod tests;
