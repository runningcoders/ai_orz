//! tests 单元测试（拆分自 fs_write.rs）
//!
//! 文件瘦身：原 533 行 → 362 行，测试体 172 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::tool_registry::tool_security::fs::is_sensitive_filename;

#[test]
fn test_validate_args_ok() {
    // overwrite with content
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: Some("content".to_string()),
        mode: "overwrite".to_string(),
        after_line: None,
        start_line: None,
        end_line: None,
    };
    assert!(validate_args(&args).is_ok());

    // insert_after requires after_line and content
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: Some("content".to_string()),
        mode: "insert_after".to_string(),
        after_line: Some(5),
        start_line: None,
        end_line: None,
    };
    assert!(validate_args(&args).is_ok());

    // delete_range requires start and end
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: None,
        mode: "delete_range".to_string(),
        after_line: None,
        start_line: Some(1),
        end_line: Some(5),
    };
    assert!(validate_args(&args).is_ok());
}

#[test]
fn test_validate_args_error() {
    // overwrite missing content
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: None,
        mode: "overwrite".to_string(),
        after_line: None,
        start_line: None,
        end_line: None,
    };
    assert!(validate_args(&args).is_err());

    // insert_after missing after_line
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: Some("content".to_string()),
        mode: "insert_after".to_string(),
        after_line: None,
        start_line: None,
        end_line: None,
    };
    assert!(validate_args(&args).is_err());

    // delete_range missing start
    let args = WriteFileArgs {
        path: "test.txt".to_string(),
        content: None,
        mode: "delete_range".to_string(),
        after_line: None,
        start_line: None,
        end_line: Some(5),
    };
    assert!(validate_args(&args).is_err());
}

#[test]
fn test_reject_sensitive_filename() {
    assert!(is_sensitive_filename(".env"));
    assert!(is_sensitive_filename("private.key"));
    assert!(is_sensitive_filename(".secrets")); // hidden file
    assert!(!is_sensitive_filename("src/lib.rs"));
    assert!(!is_sensitive_filename("tests/test.txt"));
}

#[test]
fn test_split_lines() {
    let content = "line 1\nline 2\nline 3";
    let lines = split_lines(content);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line 1");
    assert_eq!(lines[2], "line 3");
}

#[test]
fn test_split_lines_empty() {
    let content = "";
    let lines = split_lines(content);
    assert_eq!(lines.len(), 0);
}

/// 工作区身份边界：自己用户树可写；其他用户树 / 同用户其他 Agent 工作区需确认
#[tokio::test]
async fn test_write_workspace_boundary() {
    use crate::pkg::paths;
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let _ = crate::config::init();

    // 预置目录（canonicalize 要求路径存在）
    std::fs::create_dir_all(base.join("agents").join("a1")).unwrap();
    let own_ws = paths::user_agent_workspace(&base, "u1", "a1");
    let sibling_ws = paths::user_agent_workspace(&base, "u1", "a2");
    let other_user_ws = paths::user_agent_workspace(&base, "u2", "a9");
    for dir in [&own_ws, &sibling_ws, &other_user_ws] {
        std::fs::create_dir_all(dir).unwrap();
    }

    let factory = FsWriteToolFactory;
    let tool = factory.create(factory.create_po());
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    let ctx = new_test_ctx("u1", pool).to_builder().agent_id("a1").build();

    // 自己的用户工作区：写入成功
    let out = tool
        .call(
            ctx.clone(),
            serde_json::json!({
                "path": own_ws.join("note.md").to_str().unwrap(),
                "mode": "overwrite",
                "content": "hi"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("success"), Some(&serde_json::json!(true)));

    // 同用户其他 Agent 工作区：需确认
    let out = tool
        .call(
            ctx.clone(),
            serde_json::json!({
                "path": sibling_ws.join("note.md").to_str().unwrap(),
                "mode": "overwrite",
                "content": "hi"
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        out.get("require_confirmation"),
        Some(&serde_json::json!(true))
    );

    // 其他用户树：需确认
    let out = tool
        .call(
            ctx,
            serde_json::json!({
                "path": other_user_ws.join("note.md").to_str().unwrap(),
                "mode": "overwrite",
                "content": "hi"
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        out.get("require_confirmation"),
        Some(&serde_json::json!(true))
    );
}
