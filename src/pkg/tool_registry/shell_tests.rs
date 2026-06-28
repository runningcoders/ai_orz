use crate::pkg::tool_registry::shell_exec;
use common::error::Result;
use std::collections::HashMap;

#[test]
fn test_parse_config_defaults() {
    // Default config should parse correctly
    let config = serde_json::json!({});
    let shell_config: shell_exec::ShellExecConfig =
        serde_json::from_value(config).expect("Should parse empty config");
    
    // Check defaults
    assert_eq!(shell_config.default_timeout_ms(), 300_000);
    assert_eq!(shell_config.default_max_output_size_bytes(), 10 * 1024 * 1024);
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
    assert_eq!(shell_config.default_max_output_size_bytes(), 5 * 1024 * 1024);
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
        assert!(allowed_env.contains(&key), "Only allowed env should be included");
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
        assert!(!filtered.contains_key("HOME"), "Sensitive variables should be filtered out");
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
    assert_eq!(merged.get("RUSTFLAGS"), Some(&"--deny warnings".to_string()));
    assert_eq!(merged.get("TARGET"), Some(&"x86_64-unknown-linux-gnu".to_string()));
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