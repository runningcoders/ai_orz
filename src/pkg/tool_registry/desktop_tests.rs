//! desktop 单元测试（拆分自 desktop.rs，风格对齐 browser_tests.rs）

use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;

fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

#[test]
fn factory_po_metadata() {
    let po = DesktopToolFactory.create_po();
    assert_eq!(po.id, "desktop");
    assert_eq!(po.name, "Desktop Automation");
    assert_eq!(po.control_mode, ControlMode::Manual);
    assert_eq!(po.protocol, ToolProtocol::Builtin);
    // 首 tag=browser（前端与 browser 同组），次 tag=desktop（语义区分）
    assert_eq!(po.get_tags(), vec!["browser", "desktop"]);
    // D28 不变式：CLI 命令 / 运行参数进 PO config
    assert_eq!(po.cli_command().as_deref(), Some(DEFAULT_COMMAND));
    assert!(
        po.cli_install_hint()
            .is_some_and(|hint| hint.contains("agent-desktop"))
    );
    assert_eq!(po.config_timeout_ms(0), DEFAULT_TIMEOUT_MS);
    assert_eq!(po.config_max_output_bytes(0), DEFAULT_MAX_OUTPUT_BYTES);
    let cfg: DesktopConfig = serde_json::from_value(po.config.clone()).unwrap();
    assert!(cfg.disabled_commands.is_none());
    // 描述进提示词：起步引导 + snapshot-ref 循环 + 优先于裸 shell
    let desc = po.description.as_str();
    for phrase in [
        "skills get desktop",
        "skills get platform",
        "snapshot again to verify",
        "STALE_REF",
        "Prefer this tool over raw shell_exec",
    ] {
        assert!(desc.contains(phrase), "description missing '{phrase}'");
    }
    // schema 为单个整串 command
    let schema = po.parameters_schema.as_ref().unwrap();
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("command"));
    assert!(!props.contains_key("args"));
}

#[test]
fn desktop_config_roundtrip() {
    // 合法的 DesktopConfig 强类型往返（5 字段无丢失）
    let value = json!({
        "command": "agent-desktop",
        "timeout_ms": 30000,
        "max_output_bytes": 65536,
        "install_hint": "npm i -g agent-desktop",
        "disabled_commands": ["sandbox"]
    });
    let cfg: DesktopConfig = serde_json::from_value(value).unwrap();
    assert_eq!(cfg.command.as_deref(), Some("agent-desktop"));
    assert_eq!(cfg.timeout_ms, Some(30_000));
    assert_eq!(cfg.max_output_bytes, Some(65_536));
    assert_eq!(
        cfg.disabled_commands.as_deref(),
        Some(&["sandbox".to_string()][..])
    );
}

#[tokio::test]
async fn call_enforces_configured_command_denylist() {
    // 管理员显式黑名单才屏蔽，且提示引导去配置页恢复
    let mut po = DesktopToolFactory.create_po();
    let mut config: DesktopConfig = serde_json::from_value(po.config.clone()).unwrap();
    config.disabled_commands = Some(vec!["sandbox".into()]);
    po.config = serde_json::to_value(config).unwrap();
    let tool = DesktopCoreTool { po };

    let rejected = tool
        .call(
            test_ctx(),
            json!({ "command": "sandbox exec dev uname -a" }),
        )
        .await
        .unwrap();
    assert_eq!(rejected["success"], false);
    let error = rejected["error"].as_str().unwrap();
    assert!(error.contains("sandbox"), "error: {error}");
    assert!(error.contains("禁用"), "error: {error}");
}

#[tokio::test]
async fn call_allows_commands_by_default() {
    // 默认不设黑名单：sandbox 这类 CLI 原生能力也放行（本机无 CLI 会走安装引导）
    let tool = DesktopCoreTool {
        po: DesktopToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "list-apps" }))
        .await
        .unwrap();
    assert!(
        !result["error"]
            .as_str()
            .unwrap_or_default()
            .contains("已被本工具配置禁用"),
        "默认不应拒绝命令"
    );
}

#[tokio::test]
async fn call_rejects_empty_command() {
    let tool = DesktopCoreTool {
        po: DesktopToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "   " }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("不能为空"));
}

#[tokio::test]
async fn call_reports_tokenize_failure() {
    let tool = DesktopCoreTool {
        po: DesktopToolFactory.create_po(),
    };
    let result = tool
        .call(test_ctx(), json!({ "command": "type @e1 \"unterminated" }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("分词失败"));
}

#[test]
fn output_truncation_uses_combined_helper() {
    // 复用 browser 的 combine_output：UTF-8 安全 + 全量字节数
    let long = "一".repeat(200); // 每字符 3 字节，共 600 字节
    let (out, truncated, full) = combine_output(&long, "", 100);
    assert!(truncated);
    assert!(out.ends_with("... [truncated]"));
    assert_eq!(full, long.len());
    assert!(out.strip_suffix("\n\n... [truncated]").unwrap().len() <= 100);
}

#[tokio::test]
async fn persist_full_output_writes_call_id_log() {
    let dir = std::env::temp_dir().join(format!(
        "ai-orz-desktop-log-test-{}",
        chrono::Utc::now().timestamp_millis()
    ));
    let ctx = test_ctx().to_builder().tool_call_id("call-xyz").build();
    let path = persist_full_output(&dir, &ctx, "full output here")
        .await
        .unwrap();
    assert!(path.to_string_lossy().contains("desktop/logs/"));
    assert_eq!(path.file_name().unwrap(), "call-xyz.log");
    let saved = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(saved, "full output here");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn tokenize_handles_quoted_spaces_and_flag_values() {
    // shell 风格分词：引号包裹的空格是一个 token；--app Finder 分两个
    assert_eq!(
        tokenize_command("snapshot --app \"Finder Pro\" -i").unwrap(),
        vec!["snapshot", "--app", "Finder Pro", "-i"]
    );
    assert_eq!(
        tokenize_command("type @e8 \"hello world\"").unwrap(),
        vec!["type", "@e8", "hello world"]
    );
}

#[test]
fn screenshot_path_arg_skips_screen_flag_value() {
    let value_flags = ["--app", "--window-id", "--screen"];
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // 官方示例：--screen 0 的 0 是选项值，真正 PATH 是 screen.png
    assert_eq!(
        screenshot_path_arg(&args(&["--screen", "0", "screen.png"]), &value_flags),
        Some("screen.png".to_string())
    );
    // 只有 --screen 0、不带 PATH：CLI 应返回 base64，工具侧注入临时路径
    assert_eq!(
        screenshot_path_arg(&args(&["--screen", "0"]), &value_flags),
        None
    );
    // --app NAME 的值不带横线，同样要跳过
    assert_eq!(
        screenshot_path_arg(&args(&["--app", "Finder", "shot.png"]), &value_flags),
        Some("shot.png".to_string())
    );
    assert_eq!(
        screenshot_path_arg(&args(&["--", "--dash.png"]), &value_flags),
        Some("--dash.png".to_string())
    );
}
