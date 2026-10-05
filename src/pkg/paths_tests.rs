//! tests 单元测试（拆分自 paths.rs）
//!
//! 文件瘦身：原 411 行 → 181 行，测试体 231 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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

/// 调用轨迹按天分片、不按 tool_id 分目录（见 [`tool_call_trace_dir`] 的边界决策）
#[test]
fn tool_call_trace_dir_is_flat_under_tools_root() {
    assert_eq!(
        tool_call_trace_dir(Path::new("/data/.ai_orz")),
        PathBuf::from("/data/.ai_orz/tools/call_trace")
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
