//! Agent 工作区 git 基建（产物锚点基础设施）
//!
//! 版本管理全走 git（产物系统设计拍板）：Agent 工作区惰性 git init，
//! commit message 的任务/Agent 关联靠原生 hooks 而非独立 git 工具——
//! `init.templateDir` 模板挂 commit-msg hook，读 shell_exec 结构性注入的
//! `AI_ORZ_TASK_ID` / `AI_ORZ_AGENT_ID` env 追加 trailer（不可绕，
//! `--no-verify` 由任务收尾 checkpoint 兜底）。
//!
//! # 边界
//!
//! - 只管理 `base_data_path` 内的工作区；外部白名单路径不碰
//! - 已是 repo（含祖先 repo）则完全不初始化、不改任何配置
//! - 所有 git 调用走 [`crate::pkg::process::exec`]；失败 best-effort 记日志，
//!   恒不阻断主流程（工具调用/任务流转不因 git 基建失败而失败）
//!
//! # 惰性 init 粒度
//!
//! 与工作区概念对齐：working_dir 落在 `users/{uid}/agents/{aid}/work` →
//! repo 根 = Agent 工作区；落在 `users/{uid}/shared/projects/{pid}/workspace`
//! → repo 根 = 项目工作区；其余（base 内）→ working_dir 本身。

use crate::pkg::paths;
use crate::pkg::process::{self, ExecOptions};
use std::path::{Path, PathBuf};

/// commit-msg hook 追加的 trailer 键（系统侧 `git log` 按此过滤变更列表）
pub const TRAILER_TASK_ID: &str = "Task-Id";
/// commit-msg hook 追加的 trailer 键
pub const TRAILER_AGENT_ID: &str = "Agent-Id";

/// git 模板目录（base 下固定子目录，经 paths SSOT 约定）
pub fn template_dir(base_data_path: &Path) -> PathBuf {
    base_data_path.join("git").join("templates")
}

/// checkpoint 兜底提交的 message 前缀（变更列表区分正式提交 vs 兜底提交）
pub const CHECKPOINT_PREFIX: &str = "checkpoint:";

/// 默认 .gitignore 内容（屏蔽构建产物与依赖目录，工作区文件锚点不收噪声）
pub const DEFAULT_GITIGNORE: &str = "# ai-orz managed workspace ignore\nnode_modules/\ntarget/\ndist/\nbuild/\n__pycache__/\n*.log\n.DS_Store\n";

/// commit-msg hook 脚本：读 AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID 追加 trailer（幂等）
const COMMIT_MSG_HOOK: &str = r#"#!/bin/sh
# ai-orz managed hook: append Task-Id / Agent-Id trailers from injected env.
# Idempotent: skips if trailer already present. Never blocks the commit.
msg_file="$1"
[ -f "$msg_file" ] || exit 0

# no real content (all comments / blank) -> nothing to tag
grep -v '^#' "$msg_file" | grep -q '[^[:space:]]' || exit 0

# ensure trailing newline
[ -z "$(tail -c 1 "$msg_file")" ] || printf '\n' >> "$msg_file"
# ensure blank line separating trailer paragraph
[ -z "$(tail -n 1 "$msg_file")" ] || printf '\n' >> "$msg_file"

if [ -n "$AI_ORZ_TASK_ID" ] && ! grep -q '^Task-Id:' "$msg_file"; then
    printf 'Task-Id: %s\n' "$AI_ORZ_TASK_ID" >> "$msg_file"
fi
if [ -n "$AI_ORZ_AGENT_ID" ] && ! grep -q '^Agent-Id:' "$msg_file"; then
    printf 'Agent-Id: %s\n' "$AI_ORZ_AGENT_ID" >> "$msg_file"
fi
exit 0
"#;

/// 从 start 向上找最近的 git repo 根（含 worktree 的 .git 文件），不超过 base 边界
fn find_git_root(start: &Path, base: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        if dir == base {
            break;
        }
        cur = dir.parent();
    }
    None
}

/// working_dir 的 repo 根归位：Agent 工作区 / 项目工作区 / working_dir 本身
fn workspace_root_for(
    base_data_path: &Path,
    working_dir: &Path,
    user_id: Option<&str>,
    agent_id: Option<&str>,
) -> PathBuf {
    if let (Some(uid), Some(aid)) = (user_id, agent_id) {
        let agent_ws = paths::user_agent_workspace(base_data_path, uid, aid);
        if working_dir.starts_with(&agent_ws) {
            return agent_ws;
        }
        // users/{uid}/shared/projects/{pid}/... → 项目工作区
        let shared = paths::user_shared_workspace(base_data_path, uid);
        if let Ok(rel) = working_dir.strip_prefix(&shared) {
            let comps: Vec<_> = rel.components().collect();
            if comps.len() >= 2
                && comps[0].as_os_str().to_str() == Some("projects")
                && let Some(pid) = comps[1].as_os_str().to_str()
            {
                return paths::user_project_workspace(base_data_path, uid, pid);
            }
        }
    }
    working_dir.to_path_buf()
}

/// 确保 git 模板目录就绪（hooks/commit-msg 可执行）
async fn ensure_hook_template(base_data_path: &Path) -> std::io::Result<()> {
    let hooks_dir = template_dir(base_data_path).join("hooks");
    tokio::fs::create_dir_all(&hooks_dir).await?;
    let hook = hooks_dir.join("commit-msg");
    tokio::fs::write(&hook, COMMIT_MSG_HOOK).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).await?;
    }
    Ok(())
}

/// 运行 git 子命令（best-effort，错误只记日志不上抛）
async fn git_quiet(
    repo: &Path,
    args: &[&str],
    env: Vec<(String, String)>,
    context: &str,
) -> Option<process::ExecOutput> {
    let mut options = ExecOptions::new("git", args.iter().map(|a| a.to_string()).collect())
        .current_dir(repo)
        .timeout(std::time::Duration::from_secs(30));
    options.env = env;
    match process::exec(&options).await {
        Ok(output) if output.success => Some(output),
        Ok(output) => {
            sys_warn!(
                "git {context} in {} failed: {}",
                repo.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(e) => {
            sys_warn!("git {context} in {} spawn failed: {e}", repo.display());
            None
        }
    }
}

/// 工作区惰性 git init（产物锚点基建入口）
///
/// - 非 base 内路径：直接跳过（返回 None）
/// - 已在 repo 内（含祖先）：返回该 repo 根，不做任何改动
/// - 否则：按工作区归位确定 repo 根，`git init --template` 挂 hooks +
///   默认 .gitignore + repo-local user 身份（隔离 HOME 无全局 gitconfig，
///   无身份 `git commit` 会失败）
///
/// 返回 repo 根；任何失败只记日志不阻断（返回 None）。
pub async fn ensure_workspace_repo(
    base_data_path: &Path,
    working_dir: &Path,
    user_id: Option<&str>,
    agent_id: Option<&str>,
) -> Option<PathBuf> {
    if !working_dir.starts_with(base_data_path) {
        return None;
    }
    if let Some(root) = find_git_root(working_dir, base_data_path) {
        return Some(root);
    }

    let repo_root = workspace_root_for(base_data_path, working_dir, user_id, agent_id);
    // 并发场景防重复 init：git init 幂等，重复调用无害；模板保证 hooks 就位
    if let Err(e) = ensure_hook_template(base_data_path).await {
        sys_warn!("write git hook template failed: {e}");
        return None;
    }

    let tpl = template_dir(base_data_path);
    let (Some(tpl_str), Some(root_str)) = (tpl.to_str(), repo_root.to_str()) else {
        return None;
    };
    // git init 会自建目录，current_dir 固定在 base（恒存在）
    git_quiet(
        base_data_path,
        &["init", "--template", tpl_str, root_str],
        Vec::new(),
        "init",
    )
    .await
    .as_ref()?;

    // 默认 .gitignore（已存在则不覆盖用户配置）
    let gitignore = repo_root.join(".gitignore");
    if !gitignore.exists()
        && let Err(e) = tokio::fs::write(&gitignore, DEFAULT_GITIGNORE).await
    {
        sys_warn!("write default .gitignore failed: {e}");
    }

    // 初始提交：.gitignore 入库 + repo 有了 HEAD（干净基线，checkpoint/status 不受噪音干扰）
    let _ = git_quiet(
        &repo_root,
        &["add", ".gitignore"],
        Vec::new(),
        "add .gitignore",
    )
    .await;
    let _ = git_quiet(
        &repo_root,
        &["commit", "-m", "chore: init ai-orz workspace"],
        Vec::new(),
        "initial commit",
    )
    .await;

    // repo-local 身份：隔离 HOME 下无全局 gitconfig，缺身份 commit 直接失败
    let _ = git_quiet(
        &repo_root,
        &["config", "user.name", "ai-orz agent"],
        Vec::new(),
        "config user.name",
    )
    .await;
    let _ = git_quiet(
        &repo_root,
        &["config", "user.email", "agent@ai-orz.local"],
        Vec::new(),
        "config user.email",
    )
    .await;

    Some(repo_root)
}

/// 任务收尾 checkpoint 兜底提交（`checkpoint:` 前缀 + Task-Id trailer）
///
/// 工作区无变更时跳过；失败只记日志。返回是否产生了新提交。
pub async fn checkpoint_task_commit(
    repo_root: &Path,
    task_id: &str,
    agent_id: Option<&str>,
) -> bool {
    let task_env = vec![(
        crate::pkg::tool_registry::shell_policy::ENV_TASK_ID.to_string(),
        task_id.to_string(),
    )];

    // 无变更 → 跳过（避免空提交）
    let Some(status) = git_quiet(
        repo_root,
        &["status", "--porcelain"],
        task_env.clone(),
        "status",
    )
    .await
    else {
        return false;
    };
    if String::from_utf8_lossy(&status.stdout).trim().is_empty() {
        return false;
    }

    if git_quiet(repo_root, &["add", "-A"], Vec::new(), "add -A")
        .await
        .is_none()
    {
        return false;
    }

    let mut env = task_env;
    if let Some(aid) = agent_id {
        env.push((
            crate::pkg::tool_registry::shell_policy::ENV_AGENT_ID.to_string(),
            aid.to_string(),
        ));
    }
    let message = format!("{CHECKPOINT_PREFIX} task {task_id} wrapped up");
    git_quiet(
        repo_root,
        &["commit", "-m", &message],
        env,
        "checkpoint commit",
    )
    .await
    .is_some()
}

#[cfg(test)]
mod tests {
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
}
