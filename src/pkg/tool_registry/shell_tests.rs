use crate::pkg::tool_registry::shell_exec;
use std::collections::HashMap;

#[test]
fn test_parse_config_defaults() {
    // Default config should parse correctly
    let config = serde_json::json!({});
    let shell_config: shell_exec::ShellExecConfig =
        serde_json::from_value(config).expect("Should parse empty config");

    // Check defaults
    assert_eq!(shell_config.default_timeout_ms(), 300_000);
    assert_eq!(
        shell_config.default_max_output_size_bytes(),
        10 * 1024 * 1024
    );
    assert!(shell_config.additional_allowed_paths().is_empty());
    // Default allowed env should include PATH
    assert!(shell_config.allowed_env().contains(&"PATH".to_string()));
}

#[test]
fn test_parse_config_custom_values() {
    let config = serde_json::json!({
        "default_timeout_ms": 60000,
        "default_max_output_size_bytes": 5242880,
        "additional_allowed_paths": ["/projects", "/workspace"],
        "allowed_env": ["PATH", "RUSTFLAGS", "CC"]
    });

    let shell_config: shell_exec::ShellExecConfig =
        serde_json::from_value(config).expect("Should parse custom config");

    assert_eq!(shell_config.default_timeout_ms(), 60000);
    assert_eq!(
        shell_config.default_max_output_size_bytes(),
        5 * 1024 * 1024
    );
    assert_eq!(shell_config.additional_allowed_paths().len(), 2);
    assert_eq!(shell_config.allowed_env().len(), 3);
}

#[test]
fn test_filter_environment_basic() {
    let allowed_env = vec!["PATH".to_string(), "RUSTFLAGS".to_string()];
    let current_env: HashMap<String, String> = std::env::vars().collect();

    let filtered = shell_exec::filter_inherited_environment(&allowed_env);

    // PATH should be present if it exists in current env
    if current_env.contains_key("PATH") {
        assert!(filtered.contains_key("PATH"));
    }

    // Any other env vars should not be included
    for (key, _) in filtered {
        assert!(
            allowed_env.contains(&key),
            "Only allowed env should be included"
        );
    }
}

#[test]
fn test_filter_environment_filters_sensitive() {
    let allowed_env = vec!["PATH".to_string(), "HOME".to_string()];
    let current_env: HashMap<String, String> = std::env::vars().collect();

    // Only run the assertion if HOME is actually in the environment
    if current_env.contains_key("HOME") {
        let filtered = shell_exec::filter_inherited_environment(&allowed_env);
        // HOME should be filtered out even if allowed because it's sensitive
        assert!(
            !filtered.contains_key("HOME"),
            "Sensitive variables should be filtered out"
        );
    }
}

#[test]
fn test_merge_extra_environment() {
    let base = HashMap::new();
    let mut base = base;
    base.insert("PATH".to_string(), "/usr/bin".to_string());

    let extra = serde_json::json!({
        "RUSTFLAGS": "--deny warnings",
        "TARGET": "x86_64-unknown-linux-gnu"
    });

    let merged = shell_exec::merge_extra_environment(base, &extra);

    assert_eq!(merged.get("PATH"), Some(&"/usr/bin".to_string()));
    assert_eq!(
        merged.get("RUSTFLAGS"),
        Some(&"--deny warnings".to_string())
    );
    assert_eq!(
        merged.get("TARGET"),
        Some(&"x86_64-unknown-linux-gnu".to_string())
    );
}

#[test]
fn test_parse_params_basic() {
    let params = serde_json::json!({
        "command": "echo hello world",
        "background": false
    });

    let parsed: shell_exec::ShellExecParams =
        serde_json::from_value(params).expect("Should parse basic params");

    assert_eq!(parsed.command, "echo hello world");
    assert_eq!(parsed.background, Some(false));
    assert!(parsed.working_dir.is_none());
    assert!(parsed.timeout_ms.is_none());
}

#[test]
fn test_parse_params_full() {
    let params = serde_json::json!({
        "command": "cargo build",
        "working_dir": "/projects/ai_orz",
        "timeout_ms": 600000,
        "max_output_size_bytes": 5242880,
        "background": true,
        "env": {
            "RUSTFLAGS": "-C opt-level=3",
            "CC": "clang"
        }
    });

    let parsed: shell_exec::ShellExecParams =
        serde_json::from_value(params).expect("Should parse full params");

    assert_eq!(parsed.command, "cargo build");
    assert_eq!(parsed.working_dir, Some("/projects/ai_orz".to_string()));
    assert_eq!(parsed.timeout_ms, Some(600000));
    assert_eq!(parsed.max_output_size_bytes, Some(5 * 1024 * 1024));
    assert_eq!(parsed.background, Some(true));
    let env = parsed.env.as_ref().expect("env should be present");
    assert_eq!(env.get("RUSTFLAGS"), Some(&"-C opt-level=3".to_string()));
}

// ==================== 真实子进程集成测试（仅 Unix） ====================

#[cfg(unix)]
mod subprocess_tests {
    use crate::models::tool::CoreTool;
    use crate::pkg::process;
    use crate::pkg::request_context::RequestContext;
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;
    use crate::pkg::tool_registry::shell_exec::ShellExecToolFactory;

    fn setup() {
        ensure_test_base_data_path();
        // shell_exec 内部依赖 config::get().base_data_path()
        let _ = crate::config::init();
    }

    fn shell_tool() -> Box<dyn CoreTool> {
        let factory = ShellExecToolFactory;
        let po = factory.create_po();
        factory.create(po)
    }

    fn test_ctx(call_id: &str) -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
            .to_builder()
            .tool_call_id(call_id.to_string())
            .build()
    }

    /// 用户 + Agent 双上下文 ctx（Agent 为用户执行任务的场景）
    fn agent_test_ctx(call_id: &str) -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        new_test_ctx("test-user", pool)
            .to_builder()
            .agent_id("agent-x")
            .tool_call_id(call_id.to_string())
            .build()
    }

    /// 默认工作目录 = 用户树内 Agent 工作区（users/{uid}/agents/{aid}/work），
    /// HOME 注入为用户隔离 HOME（users/{uid}）
    #[tokio::test]
    async fn test_default_working_dir_and_home_follow_user_context() {
        setup();
        let base = ensure_test_base_data_path();
        let tool = shell_tool();

        // pwd 应落在用户树内 Agent 工作区（pwd 打印内核解析后的路径，先 canonicalize 期望值）
        let call_id = format!("ws-{}", uuid::Uuid::now_v7());
        let output = tool
            .call(
                agent_test_ctx(&call_id),
                serde_json::json!({ "command": "pwd" }),
            )
            .await
            .unwrap();
        assert_eq!(output.get("success"), Some(&serde_json::Value::Bool(true)));
        let out = output.get("output").and_then(|v| v.as_str()).unwrap();
        let expected_ws =
            crate::pkg::paths::user_agent_workspace(&base, "test-user", "agent-x");
        let expected_ws = expected_ws.canonicalize().unwrap_or(expected_ws);
        assert!(
            out.contains(expected_ws.to_str().unwrap()),
            "pwd='{}' should run under {}",
            out,
            expected_ws.display()
        );
        let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;
        process::registry().remove(pid);

        // $HOME 应为用户隔离 HOME（env 值原样传递，不做符号链接解析）
        let call_id = format!("ws-{}", uuid::Uuid::now_v7());
        let output = tool
            .call(
                agent_test_ctx(&call_id),
                serde_json::json!({ "command": "echo $HOME" }),
            )
            .await
            .unwrap();
        let out = output.get("output").and_then(|v| v.as_str()).unwrap();
        let expected_home = crate::pkg::paths::user_home(&base, "test-user");
        assert_eq!(out.trim(), expected_home.to_str().unwrap());
        let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;
        process::registry().remove(pid);
    }

    /// 其他用户树的工作目录：返回 require_confirmation，不执行命令
    #[tokio::test]
    async fn test_other_user_working_dir_requires_confirmation() {
        setup();
        let base = ensure_test_base_data_path();
        let tool = shell_tool();
        let other_dir =
            crate::pkg::paths::user_agent_workspace(&base, "other-user", "agent-y");

        let output = tool
            .call(
                agent_test_ctx("wd-other"),
                serde_json::json!({
                    "command": "echo hi",
                    "working_dir": other_dir.to_str().unwrap()
                }),
            )
            .await
            .unwrap();
        assert_eq!(
            output.get("success"),
            Some(&serde_json::Value::Bool(false))
        );
        assert_eq!(
            output.get("require_confirmation"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    /// 超时默认 detach：返回 timeout + pid 存活 + registry 有条目
    #[tokio::test]
    async fn test_sync_timeout_detach_keeps_process_alive() {
        setup();
        let tool = shell_tool();
        let call_id = format!("detach-{}", uuid::Uuid::now_v7());
        let args = serde_json::json!({
            "command": "sleep 5",
            "timeout_ms": 100
        });

        let output = tool.call(test_ctx(&call_id), args).await.unwrap();

        assert_eq!(
            output.get("status").and_then(|v| v.as_str()),
            Some("timeout")
        );
        assert_eq!(
            output.get("call_id").and_then(|v| v.as_str()),
            Some(call_id.as_str())
        );
        let pid = output
            .get("pid")
            .and_then(|v| v.as_u64())
            .expect("pid should be present") as u32;

        // detach 后进程仍在运行
        assert!(process::is_alive(pid), "detached process should stay alive");

        // registry 有条目且处于 Running
        let entry = process::registry()
            .get(pid)
            .expect("process should be registered");
        assert_eq!(entry.call_id, call_id);
        assert!(!entry.background);
        assert_eq!(entry.status, process::ProcessStatus::Running);

        // 日志文件以 call_id 命名
        let log_path =
            std::path::PathBuf::from(output.get("log_path").and_then(|v| v.as_str()).unwrap());
        assert_eq!(log_path.file_stem().unwrap().to_str().unwrap(), call_id);

        // 清理：终止并移除条目
        let _ = process::terminate(pid);
        process::registry().remove(pid);
    }

    /// timeout_action=kill：超时立即终止
    #[tokio::test]
    async fn test_sync_timeout_kill_terminates_process() {
        setup();
        let tool = shell_tool();
        let call_id = format!("kill-{}", uuid::Uuid::now_v7());
        let args = serde_json::json!({
            "command": "sleep 30",
            "timeout_ms": 100,
            "timeout_action": "kill"
        });

        let output = tool.call(test_ctx(&call_id), args).await.unwrap();

        assert_eq!(
            output.get("status").and_then(|v| v.as_str()),
            Some("timeout")
        );
        assert_eq!(output.get("killed"), Some(&serde_json::Value::Bool(true)));
        let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;

        // 进程已终止（SIGKILL 生效有极短窗口，轮询最多 1s）
        let mut alive = true;
        for _ in 0..20 {
            alive = process::is_alive(pid);
            if !alive {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(!alive, "killed process should not be alive");

        // registry 条目标记为已退出
        let entry = process::registry()
            .get(pid)
            .expect("process should be registered");
        assert_eq!(entry.status, process::ProcessStatus::Exited);

        process::registry().remove(pid);
    }

    /// background 模式：立即返回 + 注册 background=true + 进程存活
    #[tokio::test]
    async fn test_background_registers_process() {
        setup();
        let tool = shell_tool();
        let call_id = format!("bg-{}", uuid::Uuid::now_v7());
        let args = serde_json::json!({
            "command": "sleep 5",
            "background": true
        });

        let output = tool.call(test_ctx(&call_id), args).await.unwrap();

        assert_eq!(output.get("success"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(
            output.get("background"),
            Some(&serde_json::Value::Bool(true))
        );
        let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;
        assert!(process::is_alive(pid));

        let entry = process::registry()
            .get(pid)
            .expect("process should be registered");
        assert!(entry.background);
        assert_eq!(entry.call_id, call_id);

        // 清理
        let _ = process::terminate(pid);
        process::registry().remove(pid);
    }

    /// sync 正常完成：输出从日志文件读取，registry 标记退出码
    #[tokio::test]
    async fn test_sync_completion_reads_log_output() {
        setup();
        let tool = shell_tool();
        let call_id = format!("sync-{}", uuid::Uuid::now_v7());
        let args = serde_json::json!({ "command": "echo sync-done" });

        let output = tool.call(test_ctx(&call_id), args).await.unwrap();

        assert_eq!(output.get("success"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(output.get("exit_code"), Some(&serde_json::json!(0)));
        assert!(
            output
                .get("output")
                .and_then(|v| v.as_str())
                .is_some_and(|s| s.contains("sync-done"))
        );

        let pid = output.get("pid").and_then(|v| v.as_u64()).unwrap() as u32;
        let entry = process::registry()
            .get(pid)
            .expect("process should be registered");
        assert_eq!(entry.status, process::ProcessStatus::Exited);
        assert_eq!(entry.exit_code, Some(0));

        process::registry().remove(pid);
    }
}
