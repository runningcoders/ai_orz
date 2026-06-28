use crate::pkg::tool_registry::tool_security::fs;
use common::error::Result;
use std::path::PathBuf;

#[test]
fn resolve_and_validate_path_accepts_relative_within_base() {
    let base = PathBuf::from("/tmp/test-project");
    let result = fs::resolve_and_validate_path(&base, "src/main.rs", &[]);
    assert!(result.is_ok());
    match result.unwrap() {
        fs::ValidationResult::Valid(_) => {},
        fs::ValidationResult::NeedConfirmation(_) => {
            // Relative within base shouldn't need confirmation
            panic!("Expected valid path within base to be accepted without confirmation");
        }
    }
}

#[test]
fn resolve_and_validate_path_rejects_sensitive_files() {
    let base = PathBuf::from("/tmp/test-project");
    let result = fs::resolve_and_validate_path(&base, ".env", &[]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Access denied"));

    let result = fs::resolve_and_validate_path(&base, "id_rsa", &[]);
    assert!(result.is_err());

    let result = fs::resolve_and_validate_path(&base, ".git/config", &[]);
    assert!(result.is_err());
}

#[test]
fn resolve_and_validate_path_accepts_normal_files() {
    let base = PathBuf::from("/tmp/test-project");
    let result = fs::resolve_and_validate_path(&base, "README.md", &[]);
    assert!(result.is_ok());

    let result = fs::resolve_and_validate_path(&base, "src/lib.rs", &[]);
    assert!(result.is_ok());

    let result = fs::resolve_and_validate_path(&base, "data/file-123.txt", &[]);
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
