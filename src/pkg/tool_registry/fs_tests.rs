use crate::pkg::tool_registry::tool_security::fs;
use tempfile::TempDir;

/// Helper: create a base temp dir with subdirectories for path tests
fn create_test_base() -> TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("failed to create src dir");
    std::fs::create_dir_all(dir.path().join("data")).expect("failed to create data dir");
    dir
}

#[test]
fn resolve_and_validate_path_accepts_relative_within_base() {
    let base = create_test_base();
    let result = fs::resolve_and_validate_path(base.path(), "src/main.rs", &[]);
    assert!(result.is_ok());
    match result.unwrap() {
        fs::ValidationResult::Valid(_) => {}
        fs::ValidationResult::NeedConfirmation(_) => {
            panic!("Expected valid path within base to be accepted without confirmation");
        }
    }
}

#[test]
fn resolve_and_validate_path_rejects_sensitive_files() {
    let base = create_test_base();
    let result = fs::resolve_and_validate_path(base.path(), ".env", &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Access denied"));

    let result = fs::resolve_and_validate_path(base.path(), "id_rsa", &[]);
    assert!(result.is_err());

    let result = fs::resolve_and_validate_path(base.path(), ".git/config", &[]);
    assert!(result.is_err());
}

#[test]
fn resolve_and_validate_path_accepts_normal_files() {
    let base = create_test_base();
    let result = fs::resolve_and_validate_path(base.path(), "README.md", &[]);
    assert!(result.is_ok());

    let result = fs::resolve_and_validate_path(base.path(), "src/lib.rs", &[]);
    assert!(result.is_ok());

    let result = fs::resolve_and_validate_path(base.path(), "data/file-123.txt", &[]);
    assert!(result.is_ok());
}

#[test]
fn is_sensitive_filename_detects_correct_extensions() {
    assert!(fs::is_sensitive_filename(".env"));
    assert!(fs::is_sensitive_filename(".env.local"));
    assert!(fs::is_sensitive_filename("id_rsa"));
    assert!(fs::is_sensitive_filename("id_dsa"));
    assert!(fs::is_sensitive_filename("id_ecdsa"));
    assert!(fs::is_sensitive_filename("key.pem"));
    assert!(fs::is_sensitive_filename("cert.p12"));
    assert!(fs::is_sensitive_filename("secret.pfx"));
    assert!(fs::is_sensitive_filename(".gitignore"));
    assert!(fs::is_sensitive_filename(".hgignore"));
    assert!(fs::is_sensitive_filename(".ssh"));
    assert!(fs::is_sensitive_filename("password.txt"));
    assert!(fs::is_sensitive_filename("secret_key"));
    assert!(fs::is_sensitive_filename("api_token"));
}

#[test]
fn is_sensitive_filename_allows_non_sensitive() {
    assert!(!fs::is_sensitive_filename("README.md"));
    assert!(!fs::is_sensitive_filename("src/main.rs"));
    assert!(!fs::is_sensitive_filename("package.json"));
    assert!(!fs::is_sensitive_filename("Cargo.toml"));
    assert!(!fs::is_sensitive_filename("lib.rs"));
    assert!(!fs::is_sensitive_filename("index.html"));
    assert!(!fs::is_sensitive_filename("style.css"));
    assert!(!fs::is_sensitive_filename("app.js"));
}

#[test]
fn resolve_and_validate_path_accepts_missing_nested_path() {
    let base = create_test_base();
    // 写新文件 / 建目录：整条链都不存在也应通过校验（由调用方补建）
    let result = fs::resolve_and_validate_path(base.path(), "a/b/c.txt", &[]);
    match result.unwrap() {
        fs::ValidationResult::Valid(p) => {
            assert!(p.ends_with("a/b/c.txt"), "unexpected path: {:?}", p);
        }
        fs::ValidationResult::NeedConfirmation(_) => {
            panic!("missing nested path within base should be accepted");
        }
    }
}

#[test]
fn resolve_and_validate_path_normalizes_dotdot_within_base() {
    let base = create_test_base();
    // 词法 `..` 归一后仍指向 base 内已存在文件
    let result = fs::resolve_and_validate_path(base.path(), "src/../data/x.txt", &[]);
    assert!(matches!(result.unwrap(), fs::ValidationResult::Valid(_)));
}

#[test]
fn resolve_and_validate_path_rejects_dotdot_escape() {
    let base = create_test_base();
    // `..` 穿越出 base：必须走 NeedConfirmation，不能被词法 starts_with 绕过
    for attack in ["a/../../etc/passwd", "../../../../etc/passwd"] {
        let result = fs::resolve_and_validate_path(base.path(), attack, &[]);
        match result.unwrap() {
            fs::ValidationResult::NeedConfirmation(_) => {}
            fs::ValidationResult::Valid(p) => {
                panic!("path escape {:?} must not validate, got {:?}", attack, p)
            }
        }
    }
}

// ---- 工具级：目录列举 / 自动补建父目录 / create_directory ----

fn lazy_pool() -> sqlx::SqlitePool {
    // fs 工具不触库，pool 仅用于构造 RequestContext
    sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap()
}

/// fs_read：目录路径返回条目列表（敏感名过滤 + 类型标注 + 目录在前）
#[tokio::test]
async fn fs_read_lists_directory_entries() {
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let _ = crate::config::init();

    let proj = base.join("fs_read_list_test");
    std::fs::create_dir_all(proj.join("sub")).unwrap();
    std::fs::write(proj.join("README.md"), "# hi\n").unwrap();
    std::fs::write(proj.join("notes.txt"), "note\n").unwrap();
    std::fs::write(proj.join(".env"), "SECRET=1\n").unwrap();

    let factory = crate::pkg::tool_registry::fs_read::FsReadToolFactory;
    let tool = factory.create(factory.create_po());
    let ctx = new_test_ctx("u1", lazy_pool());

    let out = tool
        .call(ctx, serde_json::json!({ "path": proj.to_str().unwrap() }))
        .await
        .unwrap();

    assert_eq!(out.get("success"), Some(&serde_json::json!(true)));
    assert_eq!(out.get("type"), Some(&serde_json::json!("directory")));
    let entries = out.get("entries").unwrap().as_array().unwrap();
    assert_eq!(
        entries.len(),
        3,
        "sensitive .env must be filtered: {:?}",
        entries
    );
    let names: Vec<&str> = entries
        .iter()
        .map(|e| e.get("name").unwrap().as_str().unwrap())
        .collect();
    assert!(names.contains(&"README.md"));
    assert!(names.contains(&"notes.txt"));
    assert!(names.contains(&"sub"));
    assert!(!names.contains(&".env"));
    // 目录排在前面，且目录不报 size
    assert_eq!(entries[0].get("name").unwrap().as_str().unwrap(), "sub");
    assert_eq!(
        entries[0].get("type").unwrap().as_str().unwrap(),
        "directory"
    );
    assert_eq!(entries[0].get("size_bytes"), Some(&serde_json::Value::Null));
}

/// fs_read：目录 + 文件专属参数（grep 等）→ note 明示被忽略，不静默吞咽
#[tokio::test]
async fn fs_read_directory_notes_ignored_file_only_params() {
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let proj = base.join("fs_read_note_test");
    std::fs::create_dir_all(&proj).unwrap();

    let factory = crate::pkg::tool_registry::fs_read::FsReadToolFactory;
    let tool = factory.create(factory.create_po());
    let ctx = new_test_ctx("u1", lazy_pool());

    let out = tool
        .call(
            ctx,
            serde_json::json!({ "path": proj.to_str().unwrap(), "grep": "x" }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("type"), Some(&serde_json::json!("directory")));
    assert!(
        out.get("note").is_some(),
        "file-only params must be surfaced, got: {:?}",
        out
    );
}

/// fs_write：写文件自动补建缺失父目录链
#[tokio::test]
async fn fs_write_creates_missing_parent_dirs() {
    use crate::pkg::paths;
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let _ = crate::config::init();
    std::fs::create_dir_all(base.join("agents").join("a1")).unwrap();

    let factory = crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
    let tool = factory.create(factory.create_po());
    let ctx = new_test_ctx("u1", lazy_pool())
        .to_builder()
        .agent_id("a1")
        .build();

    let target = paths::agent_data_dir(&base, "a1").join("auto_mkdir_test/deep/note.md");
    let out = tool
        .call(
            ctx,
            serde_json::json!({
                "path": target.to_str().unwrap(),
                "mode": "overwrite",
                "content": "hello"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("success"), Some(&serde_json::json!(true)));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello\n");
}

/// fs_write：create_directory 建目录（含父链）且幂等
#[tokio::test]
async fn fs_write_create_directory_mode() {
    use crate::pkg::paths;
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let _ = crate::config::init();
    std::fs::create_dir_all(base.join("agents").join("a1")).unwrap();

    let factory = crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
    let tool = factory.create(factory.create_po());
    let ctx = new_test_ctx("u1", lazy_pool())
        .to_builder()
        .agent_id("a1")
        .build();

    let target = paths::agent_data_dir(&base, "a1").join("mkdir_test/a/b");

    let out = tool
        .call(
            ctx.clone(),
            serde_json::json!({
                "path": target.to_str().unwrap(),
                "mode": "create_directory"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("success"), Some(&serde_json::json!(true)));
    assert_eq!(out.get("created"), Some(&serde_json::json!(true)));
    assert!(target.is_dir());

    // 幂等：再次创建不报错，created=false
    let out = tool
        .call(
            ctx,
            serde_json::json!({
                "path": target.to_str().unwrap(),
                "mode": "create_directory"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("success"), Some(&serde_json::json!(true)));
    assert_eq!(out.get("created"), Some(&serde_json::json!(false)));
}

/// fs_write：文件模式撞上已存在目录 → 明确报错并指路（而非晦涩 EISDIR）
#[tokio::test]
async fn fs_write_to_existing_directory_errors() {
    use crate::pkg::paths;
    use crate::pkg::request_context_test_support::{ensure_test_base_data_path, new_test_ctx};
    use crate::pkg::tool_registry::BuiltinToolFactory;

    let base = ensure_test_base_data_path();
    let _ = crate::config::init();
    std::fs::create_dir_all(base.join("agents").join("a1")).unwrap();

    let factory = crate::pkg::tool_registry::fs_write::FsWriteToolFactory;
    let tool = factory.create(factory.create_po());
    let ctx = new_test_ctx("u1", lazy_pool())
        .to_builder()
        .agent_id("a1")
        .build();

    let target = paths::agent_data_dir(&base, "a1").join("dir_target_test");
    std::fs::create_dir_all(&target).unwrap();

    let out = tool
        .call(
            ctx,
            serde_json::json!({
                "path": target.to_str().unwrap(),
                "mode": "overwrite",
                "content": "x"
            }),
        )
        .await
        .unwrap();
    assert_eq!(out.get("success"), Some(&serde_json::json!(false)));
    let err = out.get("error").unwrap().as_str().unwrap();
    assert!(
        err.contains("directory"),
        "error should mention directory, got: {}",
        err
    );
}
