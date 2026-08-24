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

/// `{base}/tools/{tool_id}/call_trace` — 每次工具调用的 trace 文件目录
pub fn tool_call_trace_dir(base_data_path: &Path, tool_id: &str) -> PathBuf {
    tools_root_dir(base_data_path)
        .join(tool_id)
        .join("call_trace")
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
    fn user_project_root_under_shared_projects() {
        assert_eq!(
            user_project_root(Path::new("/data/.ai_orz"), "user-001", "proj-42"),
            PathBuf::from("/data/.ai_orz/users/user-001/shared/projects/proj-42")
        );
    }

    #[test]
    fn user_project_workspace_is_workspace_subdir() {
        assert_eq!(
            user_project_workspace(Path::new("/data/.ai_orz"), "user-001", "proj-42"),
            PathBuf::from("/data/.ai_orz/users/user-001/shared/projects/proj-42/workspace")
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
    fn users_root_dir_is_users_under_base() {
        assert_eq!(
            users_root_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/users")
        );
    }

    #[test]
    fn agent_data_dir_is_agent_rooted() {
        assert_eq!(
            agent_data_dir(Path::new("/data/.ai_orz"), "agent-007"),
            PathBuf::from("/data/.ai_orz/agents/agent-007")
        );
    }

    #[test]
    fn agent_workspace_is_work_under_agent_data_dir() {
        assert_eq!(
            agent_workspace(Path::new("/data/.ai_orz"), "agent-007"),
            PathBuf::from("/data/.ai_orz/agents/agent-007/work")
        );
    }

    #[test]
    fn agent_memory_dir_is_memory_under_agent_data_dir() {
        assert_eq!(
            agent_memory_dir(Path::new("/data/.ai_orz"), "agent-007"),
            PathBuf::from("/data/.ai_orz/agents/agent-007/memory")
        );
    }

    #[test]
    fn attachments_dir_is_top_level() {
        assert_eq!(
            attachments_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/attachments")
        );
    }

    #[test]
    fn artifacts_dir_is_top_level() {
        assert_eq!(
            artifacts_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/artifacts")
        );
    }

    #[test]
    fn artifact_project_dir_under_artifacts_projects() {
        assert_eq!(
            artifact_project_dir(Path::new("/data/.ai_orz"), "proj-42"),
            PathBuf::from("/data/.ai_orz/artifacts/projects/proj-42")
        );
    }

    #[test]
    fn artifact_path_nests_project_and_artifact() {
        assert_eq!(
            artifact_path(Path::new("/data/.ai_orz"), "proj-42", "art-9"),
            PathBuf::from("/data/.ai_orz/artifacts/projects/proj-42/art-9")
        );
    }

    #[test]
    fn vectors_dir_is_top_level() {
        assert_eq!(
            vectors_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/vectors")
        );
    }

    #[test]
    fn sqlite_db_path_joins_file_name_under_base() {
        assert_eq!(
            sqlite_db_path(Path::new("/data/.ai_orz"), "ai_orz.sqlite"),
            PathBuf::from("/data/.ai_orz/ai_orz.sqlite")
        );
    }

    #[test]
    fn vector_sqlite_db_path_joins_vector_file_name() {
        assert_eq!(
            vector_sqlite_db_path(Path::new("/data/.ai_orz"), "ai_orz_vector.sqlite"),
            PathBuf::from("/data/.ai_orz/ai_orz_vector.sqlite")
        );
    }

    #[test]
    fn lance_vector_dir_is_vectors_lance_under_base() {
        assert_eq!(
            lance_vector_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/vectors_lance")
        );
    }

    #[test]
    fn hnsw_index_dir_joins_dir_name_under_base() {
        assert_eq!(
            hnsw_index_dir(Path::new("/data/.ai_orz"), "hnsw_index"),
            PathBuf::from("/data/.ai_orz/hnsw_index")
        );
    }

    #[test]
    fn stats_db_path_joins_stats_file_name() {
        assert_eq!(
            stats_db_path(Path::new("/data/.ai_orz"), "ai_orz_stats.duckdb"),
            PathBuf::from("/data/.ai_orz/ai_orz_stats.duckdb")
        );
    }

    #[test]
    fn tools_root_dir_is_tools_under_base() {
        assert_eq!(
            tools_root_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/tools")
        );
    }

    #[test]
    fn tool_call_trace_dir_nests_tool_id_and_call_trace() {
        assert_eq!(
            tool_call_trace_dir(Path::new("/data/.ai_orz"), "web_search"),
            PathBuf::from("/data/.ai_orz/tools/web_search/call_trace")
        );
    }

    #[test]
    fn tool_logs_dir_nests_tool_id_and_logs() {
        assert_eq!(
            tool_logs_dir(Path::new("/data/.ai_orz"), "shell_exec"),
            PathBuf::from("/data/.ai_orz/tools/shell_exec/logs")
        );
    }

    #[test]
    fn skills_root_dir_is_top_level() {
        assert_eq!(
            skills_root_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/skills")
        );
    }

    #[test]
    fn shared_skill_dir_is_under_shared() {
        assert_eq!(
            shared_skill_dir(Path::new("/data/.ai_orz"), "doc-writer"),
            PathBuf::from("/data/.ai_orz/skills/shared/doc-writer")
        );
    }

    #[test]
    fn agent_skill_dir_is_under_agent_skills() {
        assert_eq!(
            agent_skill_dir(Path::new("/data/.ai_orz"), "agent-007", "doc-writer"),
            PathBuf::from("/data/.ai_orz/agents/agent-007/skills/doc-writer")
        );
    }

    #[test]
    fn seeds_dir_is_top_level() {
        assert_eq!(
            seeds_dir(Path::new("/data/.ai_orz")),
            PathBuf::from("/data/.ai_orz/seeds")
        );
    }

    #[test]
    fn default_workspace_follows_caller_identity() {
        let base = Path::new("/data/.ai_orz");
        assert_eq!(
            default_workspace(base, Some("u1"), Some("a1")),
            PathBuf::from("/data/.ai_orz/users/u1/agents/a1/work")
        );
        assert_eq!(
            default_workspace(base, None, Some("a1")),
            PathBuf::from("/data/.ai_orz/agents/a1/work")
        );
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
