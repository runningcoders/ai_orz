use super::*;
use crate::models::tool::ToolPo;
use crate::pkg::request_context::RequestContext;
use crate::pkg::request_context_test_support::new_test_ctx;

/// 测试用 ctx（懒连接内存池，不触库）
fn test_ctx(user_id: &str, agent_id: Option<&str>) -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = new_test_ctx(user_id, pool);
    match agent_id {
        Some(aid) => ctx.to_builder().agent_id(aid).build(),
        None => ctx,
    }
}

fn po_with_config(config: Value) -> ToolPo {
    let mut po = ToolPo::new(
        String::new(),
        "test-shell-tool".into(),
        "test".into(),
        common::enums::ToolProtocol::Shell,
        config,
        None,
        vec![],
        None,
    );
    po.parameters_schema = Some(json!({
        "type": "object",
        "properties": {
            "file": {"type": "string"},
            "count": {"type": "integer"}
        },
        "required": ["file"]
    }));
    po
}

fn config_value(args: Value) -> Value {
    // working_dir 不配置：运行时默认取 base_root（scope 内恒放行）
    json!({
        "program": "echo",
        "args_template": args,
        "timeout_ms": 5000
    })
}

#[test]
fn validate_config_accepts_valid() {
    let config: ShellToolConfig =
        serde_json::from_value(config_value(json!(["hello", "{{args.file}}"]))).unwrap();
    assert!(validate_config(&config).is_ok());
}

#[test]
fn validate_config_rejects_empty_program() {
    let config: ShellToolConfig = serde_json::from_value(json!({
        "program": "  ",
        "args_template": []
    }))
    .unwrap();
    assert!(validate_config(&config).is_err());
}

#[test]
fn validate_config_rejects_program_with_whitespace() {
    // program 必须是单个可执行文件，禁止 "echo foo" 这类注入
    let config: ShellToolConfig = serde_json::from_value(json!({
        "program": "echo foo",
        "args_template": []
    }))
    .unwrap();
    assert!(validate_config(&config).is_err());
}

#[test]
fn validate_config_rejects_bad_placeholder() {
    let config: ShellToolConfig = serde_json::from_value(json!({
        "program": "echo",
        "args_template": ["{{not_args.x}}"]
    }))
    .unwrap();
    assert!(validate_config(&config).is_err());
}

#[test]
fn validate_config_rejects_relative_working_dir() {
    let config: ShellToolConfig = serde_json::from_value(json!({
        "program": "echo",
        "working_dir": "relative/path"
    }))
    .unwrap();
    assert!(validate_config(&config).is_err());
}

#[test]
fn validate_config_rejects_out_of_range_timeout() {
    for ms in [0u64, MAX_TIMEOUT_MS + 1] {
        let config: ShellToolConfig = serde_json::from_value(json!({
            "program": "echo",
            "timeout_ms": ms
        }))
        .unwrap();
        assert!(validate_config(&config).is_err(), "timeout_ms={ms}");
    }
}

#[test]
fn from_po_rejects_invalid_config() {
    let po = po_with_config(json!({ "program": "" }));
    assert!(ShellCoreTool::from_po(po).is_err());
}

#[test]
fn validate_tool_po_config_rejects_missing_program() {
    let po = po_with_config(json!({}));
    assert!(validate_tool_po_config(&po).is_err());
}

#[tokio::test]
async fn call_renders_placeholders_and_executes() {
    let po = po_with_config(config_value(json!([
        "file:",
        "{{args.file}}",
        "n:",
        "{{args.count}}"
    ])));
    let tool = ShellCoreTool::from_po(po).unwrap();
    let ctx = test_ctx("test-user", None);
    let out = tool
        .call(ctx, json!({"file": "a b.txt", "count": 3}))
        .await
        .unwrap();
    assert_eq!(out["success"], json!(true));
    // 参数含空格仍作为单个 argv 项传递（不经 shell 切分）
    assert_eq!(out["stdout"], json!("file: a b.txt n: 3\n"));
    assert_eq!(out["timed_out"], json!(false));
}

#[tokio::test]
async fn call_rejects_missing_required_arg() {
    let po = po_with_config(config_value(json!(["{{args.file}}"])));
    let tool = ShellCoreTool::from_po(po).unwrap();
    let ctx = test_ctx("test-user", None);
    let result = tool.call(ctx, json!({"count": 1})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn call_rejects_unknown_arg_with_additional_properties_false() {
    let po = po_with_config(config_value(json!(["{{args.file}}"])));
    let mut po = po;
    po.parameters_schema = Some(json!({
        "type": "object",
        "properties": {"file": {"type": "string"}},
        "additionalProperties": false
    }));
    let tool = ShellCoreTool::from_po(po).unwrap();
    let ctx = test_ctx("test-user", None);
    let result = tool.call(ctx, json!({"file": "x", "evil": "y"})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn call_blocks_dangerous_working_dir_via_policy() {
    // working_dir 越过 base_data_path 边界 → shell_policy Confirm 阻断
    let mut config: ShellToolConfig = serde_json::from_value(config_value(json!(["hi"]))).unwrap();
    config.working_dir = Some("/opt/outside".into());
    let po = po_with_config(serde_json::to_value(&config).unwrap());
    let tool = ShellCoreTool::from_po(po).unwrap();
    let ctx = test_ctx("u1", Some("a1"));
    let out = tool.call(ctx, json!({"file": "x"})).await.unwrap();
    assert_eq!(out["require_confirmation"], json!(true));
}

#[tokio::test]
async fn call_reports_non_zero_exit() {
    let config = json!({
        "program": "sh",
        "args_template": ["-c", "exit 7"],
        "timeout_ms": 5000
    });
    let po = po_with_config(config);
    let tool = ShellCoreTool::from_po(po).unwrap();
    let ctx = test_ctx("test-user", None);
    let out = tool.call(ctx, json!({"file": "x"})).await.unwrap();
    assert_eq!(out["success"], json!(false));
    assert_eq!(out["exit_code"], json!(7));
}
