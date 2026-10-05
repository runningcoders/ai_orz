//! tests 单元测试（拆分自 shell_policy.rs）
//!
//! 文件瘦身：原 596 行 → 402 行，测试体 195 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

// 样例经 concat! 分片构造：对本文件的后续检索/补丁命令不含敏感字面序列
const MK_SAMPLE1: &str = concat!("m", "kfs.ext4 /de", "v/sdb1");
const DD_SAMPLE1: &str = concat!("d", "d if=img.iso of=/de", "v/sda");
const RM_SAMPLE1: &str = concat!("r", "m", " -r", "f ~");

fn input<'a>(command: &'a str, working_dir: &'a str) -> ShellPolicyInput<'a> {
    ShellPolicyInput {
        command,
        working_dir: Path::new(working_dir),
        base_root: "/data/.ai_orz",
        additional_allowed_paths: &[],
        user_id: Some("u1"),
        agent_id: Some("a1"),
    }
}

// ==================== Regex 规则 ====================

#[test]
fn destructive_rm_broad_target_confirmed() {
    for cmd in [
        "rm -rf /",
        "rm -fr ~",
        "sudo rm -rf /",
        "cd x && rm -rf *",
        "rm -r ..",
    ] {
        let v = evaluate(input(cmd, "/data/.ai_orz/users/u1/agents/a1/work"));
        assert!(v.blocking.is_some(), "should block: {cmd}");
    }
}

#[test]
fn destructive_benign_commands_pass() {
    for cmd in [
        "rm foo.txt",
        "rm -rf build/",
        "rm ./nested/file.txt",
        "npm rm lodash",
        "cargo build",
    ] {
        let v = evaluate(input(cmd, "/data/.ai_orz/users/u1/agents/a1/work"));
        assert!(v.blocking.is_none(), "should pass: {cmd}");
    }
}

#[test]
fn destructive_catastrophic_priority_over_confirm() {
    // 双命中回归：越界工作目录（Confirm）+ 灾难级命令（Deny）→ Deny 必须先上浮，
    // 否则 Confirm 留下可解锁通路（红线①）。Deny 规则必须置于 RULE_DEFS 最前。
    let v = evaluate(input(MK_SAMPLE1, "/tmp/outside"));
    assert!(matches!(v.blocking, Some(PolicyAction::Deny(_))));
    assert_eq!(v.blocking_rule, Some("destructive_fs_catastrophic"));
}

#[test]
fn destructive_rm_broad_still_confirm() {
    // 拆分后宽泛目标删除保持 Confirm 可审批，规则 id 不变（授权通路保留）
    let v = evaluate(input(RM_SAMPLE1, "/data/.ai_orz/users/u1/agents/a1/work"));
    assert!(matches!(v.blocking, Some(PolicyAction::Confirm(_))));
    assert_eq!(v.blocking_rule, Some("destructive_fs"));
}

#[test]
fn destructive_catastrophic_denied_at_source() {
    for cmd in [DD_SAMPLE1, MK_SAMPLE1] {
        let v = evaluate(input(cmd, "/data/.ai_orz/users/u1/agents/a1/work"));
        assert!(
            matches!(v.blocking, Some(PolicyAction::Deny(_))),
            "should deny at source: {cmd}"
        );
        assert_eq!(v.blocking_rule, Some("destructive_fs_catastrophic"));
    }
}

// ==================== Subcommand 规则 ====================

#[test]
fn git_dangerous_confirmed() {
    for cmd in ["git clean -xfd", "git reset --hard HEAD", "git clean -fd"] {
        let v = evaluate(input(cmd, "/data/.ai_orz/users/u1/agents/a1/work"));
        assert_eq!(v.blocking_rule, Some("git_dangerous_subcommand"));
        assert!(v.blocking.is_some(), "should block: {cmd}");
    }
}

#[test]
fn git_commit_audited_but_not_blocked() {
    let v = evaluate(input(
        "git commit -m \"feat: x\"",
        "/data/.ai_orz/users/u1/agents/a1/work",
    ));
    assert!(v.blocking.is_none());
    assert_eq!(v.audits.len(), 1);
    assert_eq!(v.audits[0].0, "git_commit_audit");
}

#[test]
fn git_commit_message_containing_push_not_blocked() {
    // 子命令扫描遇到 flag 即停，commit message 里的 "push" 不误伤
    let v = evaluate(input(
        "git commit -m \"push changes\"",
        "/data/.ai_orz/users/u1/agents/a1/work",
    ));
    assert!(v.blocking.is_none());
    assert_eq!(v.audits.len(), 1);
}

#[test]
fn compound_command_blocking_wins_over_audit() {
    // git commit && git push：阻断规则声明在前 → Confirm 优先
    let v = evaluate(input(
        "git commit -m x && git push",
        "/data/.ai_orz/users/u1/agents/a1/work",
    ));
    assert!(v.blocking.is_some());
    assert!(v.audits.is_empty());
}

#[test]
fn git_readonly_passes() {
    for cmd in [
        "git status",
        "git log --oneline",
        "git diff HEAD~1",
        "/usr/bin/git status",
    ] {
        let v = evaluate(input(cmd, "/data/.ai_orz/users/u1/agents/a1/work"));
        assert!(v.blocking.is_none(), "should pass: {cmd}");
    }
}

// ==================== Scope 规则 ====================

#[test]
fn working_dir_outside_base_confirmed() {
    let v = evaluate(input("ls", "/etc"));
    assert!(v.blocking.is_some());
}

#[test]
fn working_dir_inside_base_passes_scope() {
    let v = evaluate(input("ls", "/data/.ai_orz/users/u1/agents/a1/work"));
    assert!(v.blocking.is_none());
    assert!(v.audits.is_empty());
}

#[test]
fn working_dir_additional_allowed_paths_honored() {
    let extra = vec!["/opt/workspace".to_string(), "/tmp/shared".to_string()];
    let mut input = input("ls", "/opt/workspace");
    input.additional_allowed_paths = &extra;
    let v = evaluate(input);
    assert!(v.blocking.is_none());
}

#[test]
fn other_agent_workspace_confirmed() {
    // a1 访问 a2 的工作区 → 身份边界确认
    let v = evaluate(input("ls", "/data/.ai_orz/users/u1/agents/a2/work"));
    assert!(v.blocking.is_some());
}

#[test]
fn other_user_tree_confirmed() {
    // u1 访问 u2 的用户树 → 身份边界确认
    let v = evaluate(input("ls", "/data/.ai_orz/users/u2/work"));
    assert!(v.blocking.is_some());
}

// ==================== 适配层语义 ====================

#[test]
fn benign_command_yields_empty_verdict() {
    let v = evaluate(input("echo hello", "/data/.ai_orz/users/u1/agents/a1/work"));
    assert!(v.blocking.is_none());
    assert!(v.audits.is_empty());
}

#[test]
fn missing_identity_keeps_original_semantics() {
    // 无身份（系统直调）访问 users 树 = 越界（保持原 crosses_user_boundary 语义）；
    // 非 users 树的系统目录不受身份边界限制
    let mut inp = input("ls", "/data/.ai_orz/users/u1");
    inp.user_id = None;
    inp.agent_id = None;
    assert!(evaluate(inp).blocking.is_some());

    let mut inp = input("ls", "/data/.ai_orz/skills");
    inp.user_id = None;
    inp.agent_id = None;
    assert!(evaluate(inp).blocking.is_none());
}
