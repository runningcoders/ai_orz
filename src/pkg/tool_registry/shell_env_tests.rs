use super::*;
use std::collections::HashMap;

#[test]
fn resolve_injects_identity_and_completes_path() {
    let mut extra = HashMap::new();
    extra.insert("RUSTFLAGS".to_string(), "--deny warnings".to_string());

    let env = resolve(&ShellEnvRequest {
        allowed_env: &["PATH".to_string()],
        path_additions: None,
        extra_env: Some(&extra),
        task_id: Some("task-1"),
        agent_id: Some("agent-1"),
    });

    assert_eq!(
        env.get("RUSTFLAGS").map(String::as_str),
        Some("--deny warnings")
    );
    assert_eq!(
        env.get("AI_ORZ_TASK_ID").map(String::as_str),
        Some("task-1")
    );
    assert_eq!(
        env.get("AI_ORZ_AGENT_ID").map(String::as_str),
        Some("agent-1")
    );
    // PATH 恒被补全（以父进程 PATH 为起点）
    let path = env.get("PATH").expect("PATH should be injected");
    assert!(path.contains("/usr/bin"), "unexpected PATH: {path}");
}

#[test]
fn resolve_without_identity_omits_identity_vars() {
    let env = resolve(&ShellEnvRequest::default());
    assert!(!env.contains_key("AI_ORZ_TASK_ID"));
    assert!(!env.contains_key("AI_ORZ_AGENT_ID"));
}

#[test]
fn filter_respects_allow_list_and_blocks_sensitive() {
    let filtered = filter_inherited_environment(&["PATH".to_string(), "HOME".to_string()]);
    for key in filtered.keys() {
        assert_eq!(key, "PATH", "sensitive/未放行变量应被剔除: {key}");
    }
}

#[test]
fn completed_path_appends_existing_directories_only() {
    let base = "/usr/bin:/bin";
    let completed = completed_path(Some(base), &default_path_additions()).expect("PATH exists");
    // 起点顺序保持在前
    assert!(completed.starts_with("/usr/bin:/bin"));
    // 默认补全目录里存在的会被追加到尾部，且不重复
    for dir in std::env::split_paths(&completed) {
        assert!(dir.is_dir(), "不存在的目录不应进入 PATH: {}", dir.display());
    }
    assert_eq!(
        completed.matches("/usr/bin").count(),
        1,
        "既有目录不应重复: {completed}"
    );
}

#[test]
fn completed_path_appends_default_homebrew_dir_when_present() {
    let completed =
        completed_path(Some("/usr/bin"), &default_path_additions()).expect("PATH exists");
    let entries: Vec<String> = std::env::split_paths(&completed)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.first().map(String::as_str), Some("/usr/bin"));
    // 机器上有 homebrew 时应在尾部；没有则不应凭空出现
    if PathBuf::from("/opt/homebrew/bin").is_dir() {
        assert!(entries.contains(&"/opt/homebrew/bin".to_string()));
    }
}

#[test]
fn expand_entry_expands_tilde() {
    let expanded = expand_entry("~");
    assert_eq!(expanded.len(), 1);
    assert!(expanded[0].is_dir());
}

#[test]
fn expand_entry_handles_wildcard_missing_gracefully() {
    // 不存在的通配父目录：返回空而不是 panic
    assert!(expand_entry("/nonexistent-root-xyz/*/bin").is_empty());
    // 空条目与空白条目
    assert!(expand_entry("").is_empty());
    assert!(expand_entry("   ").is_empty());
}

#[test]
fn resolve_honors_tool_level_path_additions() {
    // 工具级（ToolPo.config）声明的目录应覆盖内置默认并进入 PATH
    let dir = tempfile::tempdir().expect("temp dir");
    let additions = vec![dir.path().to_string_lossy().into_owned()];
    let env = resolve(&ShellEnvRequest {
        path_additions: Some(&additions),
        ..Default::default()
    });
    let path = env.get("PATH").expect("PATH should be injected");
    let entries: Vec<String> = std::env::split_paths(path)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        entries.contains(&dir.path().to_string_lossy().into_owned()),
        "工具级 path_additions 应进入 PATH: {path}"
    );
}

#[test]
fn home_for_isolated_returns_user_home() {
    let mode = HomeMode::Isolated;
    let home = home_for(mode, Some("u1"), Path::new("/data/.ai_orz"));
    assert_eq!(
        home,
        Some(crate::pkg::paths::user_home(
            Path::new("/data/.ai_orz"),
            "u1"
        ))
    );
}

#[test]
fn home_for_inherit_falls_back_to_parent_process() {
    assert_eq!(
        home_for(HomeMode::Inherit, Some("u1"), Path::new("/data")),
        None
    );
    // 无 user_id 时无法定位隔离 HOME，同样保持继承
    assert_eq!(home_for(HomeMode::Isolated, None, Path::new("/data")), None);
}

#[test]
fn merge_extra_environment_overrides_base() {
    let mut base = HashMap::new();
    base.insert("PATH".to_string(), "/usr/bin".to_string());
    let extra = serde_json::json!({ "RUSTFLAGS": "--deny warnings" });
    let merged = merge_extra_environment(base, &extra);
    assert_eq!(merged.get("PATH").map(String::as_str), Some("/usr/bin"));
    assert_eq!(
        merged.get("RUSTFLAGS").map(String::as_str),
        Some("--deny warnings")
    );
}

#[test]
fn toolchain_injections_map_existing_dirs_to_official_vars() {
    let home = tempfile::tempdir().expect("temp home");
    std::fs::create_dir_all(home.path().join(".cargo")).expect("create .cargo");
    std::fs::create_dir_all(home.path().join(".nvm")).expect("create .nvm");

    let injections = toolchain_env_injections_with(
        &["cargo".to_string(), "NVM".to_string(), "pyenv".to_string()],
        Some(home.path()),
    );

    // 存在的目录映射到官方变量（大小写不敏感）
    let cargo_home = injections
        .iter()
        .find(|(key, _)| key == "CARGO_HOME")
        .expect("cargo dir exists → CARGO_HOME");
    assert_eq!(
        Path::new(cargo_home.1.as_str()),
        &home.path().join(".cargo")
    );
    assert!(
        injections.iter().any(|(key, _)| key == "NVM_DIR"),
        "nvm 大小写不敏感"
    );
    // pyenv 目录不存在 → 跳过；未知名忽略
    assert!(!injections.iter().any(|(key, _)| key == "PYENV_ROOT"));
    assert_eq!(
        injections.len(),
        2,
        "未知名与缺失路径都不产出: {injections:?}"
    );
}

#[test]
fn toolchain_injections_empty_without_real_home() {
    let injections = toolchain_env_injections_with(&["cargo".to_string()], None);
    assert!(injections.is_empty());
}

#[test]
fn git_ssh_command_injection_requires_agent_and_known_hosts() {
    let home = tempfile::tempdir().expect("temp home");
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).expect("create .ssh");

    // known_hosts 还没建 → 不注入
    assert!(git_ssh_command_injection_with(true, Some(home.path())).is_none());

    // 建好 known_hosts → 注入，指向真实路径
    std::fs::write(ssh_dir.join("known_hosts"), "").expect("write known_hosts");
    let injected = git_ssh_command_injection_with(true, Some(home.path()))
        .expect("agent + known_hosts → 注入");
    assert!(injected.starts_with("ssh -o UserKnownHostsFile="));
    assert!(injected.contains("StrictHostKeyChecking=accept-new"));
    let expected_path = home
        .path()
        .join(".ssh")
        .join("known_hosts")
        .to_string_lossy()
        .into_owned();
    assert!(
        injected.contains(expected_path.as_str()),
        "known_hosts 应指向真实 HOME 路径: {injected}"
    );
    // 绝不注入私钥路径
    assert!(!injected.contains("-i "), "禁止 -i 私钥: {injected}");

    // 无 agent → 不注入（key 无从谈起）
    assert!(git_ssh_command_injection_with(false, Some(home.path())).is_none());
    // 无真实 HOME → 不注入
    assert!(git_ssh_command_injection_with(true, None).is_none());
}

#[test]
fn git_ssh_command_injection_quotes_special_paths() {
    // HOME 根路径含空格（tempfile 前缀可含空格；函数固定拼 real_home/.ssh/known_hosts）
    let home = tempfile::Builder::new()
        .prefix("ssh dir with space ")
        .tempdir()
        .expect("temp home");
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh_dir).expect("create .ssh");
    std::fs::write(ssh_dir.join("known_hosts"), "").expect("write known_hosts");
    let injected =
        git_ssh_command_injection_with(true, Some(home.path())).expect("known_hosts exists");
    // 路径含空格 → shell 单引号包裹，命令仍可被 shell 正确解析
    assert!(injected.contains("'"), "路径需 shell 引用: {injected}");
}

#[test]
fn common_toolchain_list_has_unique_names_and_vars() {
    use common::models::tool::SHELL_TOOLCHAIN_HOME_VARS;
    let mut names: Vec<_> = SHELL_TOOLCHAIN_HOME_VARS.iter().map(|e| e.0).collect();
    let mut vars: Vec<_> = SHELL_TOOLCHAIN_HOME_VARS.iter().map(|e| e.1).collect();
    names.sort_unstable();
    vars.sort_unstable();
    assert!(
        names.windows(2).all(|w| w[0] != w[1]),
        "工具链名不应重复: {names:?}"
    );
    assert!(
        vars.windows(2).all(|w| w[0] != w[1]),
        "官方变量名不应重复: {vars:?}"
    );
}
