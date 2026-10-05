//! tests 单元测试（拆分自 git_workspace.rs）
//!
//! 文件瘦身：原 479 行 → 295 行，测试体 185 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn find_git_root_walks_up_within_base() {
    let base = std::env::temp_dir().join("ai-orz-gw-test-base");
    let repo = base.join("users/u1/agents/a1/work");
    let nested = repo.join("sub/dir");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    // repo 内：找到 repo 根
    assert_eq!(find_git_root(&nested, &base), Some(repo.clone()));
    // repo 外（base 顶层）：None
    let outside = base.join("skills");
    std::fs::create_dir_all(&outside).unwrap();
    assert_eq!(find_git_root(&outside, &base), None);
    // base 外：None
    assert_eq!(find_git_root(Path::new("/etc"), Path::new("/data")), None);

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn workspace_root_resolves_agent_and_project_workspaces() {
    let base = Path::new("/data/.ai_orz");

    // Agent 工作区内嵌套目录 → repo 根归位到 Agent 工作区
    let agent_ws = paths::user_agent_workspace(base, "u1", "a1");
    let nested = agent_ws.join("sub");
    assert_eq!(
        workspace_root_for(base, &nested, Some("u1"), Some("a1")),
        agent_ws
    );

    // 项目工作区 → repo 根归位到项目工作区
    let proj_ws = paths::user_project_workspace(base, "u1", "p1");
    let nested = proj_ws.join("src");
    assert_eq!(
        workspace_root_for(base, &nested, Some("u1"), Some("a1")),
        proj_ws
    );

    // 其余路径 → working_dir 本身
    assert_eq!(
        workspace_root_for(base, &base.join("skills"), Some("u1"), Some("a1")),
        base.join("skills")
    );
}

#[tokio::test]
async fn ensure_repo_skips_outside_base() {
    assert!(
        ensure_workspace_repo(Path::new("/data"), Path::new("/etc"), None, None)
            .await
            .is_none()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn ensure_repo_initializes_with_hook_and_gitignore() {
    let base = tempfile::tempdir().unwrap();
    let ws = base.path().join("users/u1/agents/a1/work");

    let root = ensure_workspace_repo(base.path(), &ws, Some("u1"), Some("a1"))
        .await
        .expect("workspace repo should be ensured");
    assert_eq!(root, ws);
    assert!(ws.join(".git").exists());
    assert!(ws.join(".gitignore").exists());

    // 已是 repo：幂等返回同一根
    let again = ensure_workspace_repo(base.path(), &ws, Some("u1"), Some("a1"))
        .await
        .expect("existing repo should be detected");
    assert_eq!(again, ws);

    // hook 模板就位且可执行
    let hook = template_dir(base.path()).join("hooks/commit-msg");
    assert!(hook.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert!(hook.metadata().unwrap().permissions().mode() & 0o111 != 0);
    }

    // repo-local 身份已配置
    let out = process::exec(
        &ExecOptions::new("git", vec!["config".into(), "user.name".into()]).current_dir(&ws),
    )
    .await
    .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ai-orz agent");
}

#[cfg(unix)]
#[tokio::test]
async fn checkpoint_commits_with_trailer_and_skips_clean_repo() {
    let base = tempfile::tempdir().unwrap();
    let ws = base.path().join("users/u1/agents/a1/work");
    ensure_workspace_repo(base.path(), &ws, Some("u1"), Some("a1"))
        .await
        .expect("repo ensured");

    // 干净 repo：无提交
    assert!(!checkpoint_task_commit(&ws, "task-1", Some("a1")).await);

    // 有变更：产生带 trailer 的 checkpoint 提交
    std::fs::write(ws.join("out.txt"), "hello").unwrap();
    assert!(checkpoint_task_commit(&ws, "task-1", Some("a1")).await);

    let log = process::exec(
        &ExecOptions::new("git", vec!["log".into(), "--format=%B".into(), "-1".into()])
            .current_dir(&ws),
    )
    .await
    .unwrap();
    let message = String::from_utf8_lossy(&log.stdout);
    assert!(message.contains("checkpoint: task task-1"));
    assert!(message.contains("Task-Id: task-1"));
    assert!(message.contains("Agent-Id: a1"));

    // 再跑一次：无变更，不产生空提交（初始提交 + checkpoint = 2）
    assert!(!checkpoint_task_commit(&ws, "task-1", Some("a1")).await);
    let count = process::exec(
        &ExecOptions::new(
            "git",
            vec!["rev-list".into(), "--count".into(), "HEAD".into()],
        )
        .current_dir(&ws),
    )
    .await
    .unwrap();
    assert_eq!(String::from_utf8_lossy(&count.stdout).trim(), "2");
}

#[cfg(unix)]
#[tokio::test]
async fn commit_msg_hook_injects_trailers_via_env() {
    // 端到端：agent 侧 git commit（env 注入）→ hook 追加 trailer
    let base = tempfile::tempdir().unwrap();
    let ws = base.path().join("users/u1/agents/a1/work");
    ensure_workspace_repo(base.path(), &ws, Some("u1"), Some("a1"))
        .await
        .expect("repo ensured");

    std::fs::write(ws.join("feat.txt"), "code").unwrap();
    let out = process::exec(
        &ExecOptions::new("git", vec!["add".into(), "feat.txt".into()]).current_dir(&ws),
    )
    .await
    .unwrap();
    assert!(
        out.success,
        "add should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = process::exec(
        &ExecOptions::new(
            "git",
            vec!["commit".into(), "-m".into(), "feat: add feature".into()],
        )
        .current_dir(&ws)
        .env("AI_ORZ_TASK_ID", "task-42")
        .env("AI_ORZ_AGENT_ID", "a1"),
    )
    .await
    .unwrap();
    assert!(
        out.success,
        "commit should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let log = process::exec(
        &ExecOptions::new("git", vec!["log".into(), "--format=%B".into(), "-1".into()])
            .current_dir(&ws),
    )
    .await
    .unwrap();
    let message = String::from_utf8_lossy(&log.stdout);
    assert!(message.contains("feat: add feature"));
    assert!(message.contains("Task-Id: task-42"));
    assert!(message.contains("Agent-Id: a1"));
}
