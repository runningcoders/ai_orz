//! tests 单元测试（拆分自 browser.rs）
//!
//! 文件瘦身：原 579 行 → 433 行，测试体 147 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;

fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

#[test]
fn factory_po_metadata() {
    let po = BrowserToolFactory.create_po();
    assert_eq!(po.id, "browser");
    assert_eq!(po.control_mode, ControlMode::Manual);
    assert_eq!(po.protocol, ToolProtocol::Builtin);
    assert_eq!(po.get_tags(), vec!["browser", "network"]);
    // CLI 命令与运行参数进 PO config（D28 不变式：CLI 型 = po.config.command）
    assert_eq!(po.cli_command().as_deref(), Some(DEFAULT_COMMAND));
    assert!(
        po.cli_install_hint()
            .is_some_and(|hint| hint.contains("agent-browser"))
    );
    assert_eq!(po.config_timeout_ms(0), DEFAULT_TIMEOUT_MS);
    assert_eq!(po.config_max_output_bytes(0), DEFAULT_MAX_OUTPUT_BYTES);
}

#[test]
fn whitelist_excludes_dangerous_commands() {
    for dangerous in [
        "eval", "batch", "chat", "connect", "cookies", "storage", "upload", "network", "set",
        "stream", "keyboard",
    ] {
        assert!(
            !ALLOWED_COMMANDS.contains(&dangerous),
            "{} must not be whitelisted",
            dangerous
        );
    }
    for required in [
        "open",
        "read",
        "snapshot",
        "click",
        "fill",
        "type",
        "press",
        "scroll",
        "screenshot",
        "wait",
        "close",
    ] {
        assert!(
            ALLOWED_COMMANDS.contains(&required),
            "{} must be whitelisted",
            required
        );
    }
}

#[tokio::test]
async fn call_rejects_non_whitelisted_command() {
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "eval", "args": ["alert(1)"] }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    let error = result["error"].as_str().unwrap();
    assert!(
        error.contains("eval"),
        "error should name the rejected command: {}",
        error
    );
    assert!(
        error.contains("白名单"),
        "error should mention whitelist: {}",
        error
    );
}

#[tokio::test]
async fn call_rejects_close_all() {
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "close", "args": ["--all"] }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("--all"));
}

#[tokio::test]
async fn call_rejects_screenshot_positional_path() {
    let tool = BrowserCoreTool {
        po: BrowserToolFactory.create_po(),
    };
    // Agent 传任意路径（路径穿越风险面）→ 拒绝
    let result = tool
        .call(
            test_ctx(),
            json!({ "command": "screenshot", "args": ["/etc/evil.png"] }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("路径"));
}

#[tokio::test]
async fn session_id_isolates_per_agent() {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let storage = crate::pkg::storage::test_support::create_test_storage(pool.clone());
    let ctx_agent = RequestContext::builder()
        .user_id("u-1".to_string())
        .agent_id("agent-9".to_string())
        .storage(storage.clone())
        .build();
    let ctx_user_only = RequestContext::builder()
        .user_id("u-2".to_string())
        .storage(storage)
        .build();
    let ctx_anon = new_test_ctx("", pool);

    assert_eq!(session_id(&ctx_agent), "ai-orz-agent-agent-9");
    assert_eq!(session_id(&ctx_user_only), "ai-orz-agent-u-2");
    assert!(session_id(&ctx_anon).starts_with("ai-orz-agent-"));
}

#[test]
fn output_truncation() {
    let short = "hello";
    let (out, truncated) = combine_output(short, "", 1024);
    assert_eq!(out, short);
    assert!(!truncated);

    let long = "x".repeat(300);
    let (out, truncated) = combine_output(&long, "", 100);
    assert!(truncated);
    assert!(out.ends_with("[truncated]"));
    assert!(out.len() <= 100 + 20);
}
