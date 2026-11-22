use std::collections::HashMap;

use super::browser_env_probe::{
    build_recommendations, merge_catalog, parse_env_block, parse_env_line, sanitize_proxy,
};

const SAMPLE_DOC: &str = r#"
## Global Options

Some unrelated section.

## Environment Variables

```bash
AGENT_BROWSER_EXECUTABLE_PATH="/path/chrome" # Custom browser path
AGENT_BROWSER_SESSION="mysession"            # Default session name
AGENT_BROWSER_NO_VERSION_CHECK="1"           # Skip the outdated-Chrome CVE check
AGENT_BROWSER_WEBGPU="1"                     # Enable the WebGPU launch preset
BROWSER_USE_API_KEY="your-api-key"           # Browser Use Cloud API key
AGENT_BROWSER_PLUGINS='[{"name":"vault"}]'   # Plugin config
```
"#;

#[test]
fn parse_env_block_reads_example_and_desc() {
    let parsed = parse_env_block(SAMPLE_DOC);
    // 风险项（SESSION / PLUGINS）照常进目录供配置页标注提示，只过滤
    // 非 AGENT_BROWSER_ 前缀项（BROWSER_USE_API_KEY）——前缀是唯一硬约束。
    assert_eq!(
        parsed.len(),
        5,
        "前缀内项全保留、非 AGENT_BROWSER_ 前缀项过滤"
    );
    assert_eq!(parsed[0].key, "AGENT_BROWSER_EXECUTABLE_PATH");
    assert_eq!(parsed[0].example, "/path/chrome");
    assert_eq!(parsed[0].desc, "Custom browser path");
    assert_eq!(parsed[1].key, "AGENT_BROWSER_SESSION");
    assert_eq!(parsed[2].key, "AGENT_BROWSER_NO_VERSION_CHECK");
    assert_eq!(parsed[3].key, "AGENT_BROWSER_WEBGPU");
    assert_eq!(parsed[4].key, "AGENT_BROWSER_PLUGINS");
}

#[test]
fn parse_env_line_ignores_non_agent_browser_keys() {
    // 前缀过滤在解析层（唯一硬约束）；风险项照常保留，仅在 merge_catalog 附风险说明。
    for line in [
        "BROWSER_USE_API_KEY=\"key\" # provider key",
        "PATH=\"/tmp\" # system env",
        "not-an-assignment",
        "# comment only",
    ] {
        assert!(parse_env_line(line).is_none(), "must ignore: {line}");
    }
    // 危险项能被解析出来，但将在 merge_catalog 里被标为不可配置
    let headed = parse_env_line("AGENT_BROWSER_HEADED=\"1\" # break headless").unwrap();
    assert_eq!(headed.key, "AGENT_BROWSER_HEADED");
}

#[test]
fn merge_catalog_keeps_static_first_and_marks_source() {
    let discovered = parse_env_block(SAMPLE_DOC);
    let mut recommendations = HashMap::new();
    recommendations.insert(
        "AGENT_BROWSER_EXECUTABLE_PATH".to_string(),
        vec![common::api::BrowserEnvOptionDTO {
            value: "/bin/fake".to_string(),
            label: "Fake browser".to_string(),
        }],
    );
    let dto = merge_catalog(Some("0.43.0".to_string()), discovered, recommendations);

    assert_eq!(dto.cli_version.as_deref(), Some("0.43.0"));
    // 18 兜底 + WEBGPU + SESSION / PLUGINS（风险项保留展示）
    assert_eq!(dto.entries.len(), 21);
    assert_eq!(dto.entries[0].key, "AGENT_BROWSER_EXECUTABLE_PATH");
    assert!(!dto.entries[0].advanced);
    assert!(dto.entries[0].from_cli);
    assert!(dto.entries[0].configurable);
    assert_eq!(dto.entries[0].recommended[0].value, "/bin/fake");

    // 风险项带 configurable=true + 风险说明进 DTO：可填，只是配置页提示
    let session = dto
        .entries
        .iter()
        .find(|e| e.key == "AGENT_BROWSER_SESSION")
        .unwrap();
    assert!(session.from_cli);
    assert!(session.configurable);
    assert!(session.advanced);
    assert!(session.desc.contains("风险提示"));
    let plugins = dto
        .entries
        .iter()
        .find(|e| e.key == "AGENT_BROWSER_PLUGINS")
        .unwrap();
    assert!(plugins.configurable);

    let webgpu = dto
        .entries
        .iter()
        .find(|e| e.key == "AGENT_BROWSER_WEBGPU")
        .unwrap();
    assert!(webgpu.from_cli);
    assert!(webgpu.configurable);
    assert_eq!(webgpu.label, "webgpu");
    assert!(webgpu.choices.is_empty());
}

#[test]
fn sanitize_proxy_drops_credentials_and_unsupported_schemes() {
    assert_eq!(
        sanitize_proxy("http://127.0.0.1:7890"),
        Some("http://127.0.0.1:7890".to_string())
    );
    assert_eq!(sanitize_proxy("http://user:pass@proxy.example:8080"), None);
    assert_eq!(sanitize_proxy("file:///etc/passwd"), None);
    assert_eq!(sanitize_proxy("not a url"), None);
}

#[test]
fn build_recommendations_never_panics_without_browser() {
    let _ = build_recommendations();
}

/// 手工验收用：本机装有 agent-browser 时跑 `cargo test ... -- --ignored`
#[tokio::test]
#[ignore = "requires locally installed agent-browser"]
async fn probe_real_cli_returns_dynamic_catalog() {
    let dto = super::browser_env_probe::probe(None)
        .await
        .expect("agent-browser must be installed");
    assert!(dto.cli_version.is_some(), "CLI 版本应被解析出来");
    assert!(dto.entries.len() >= 18, "静态兜底项必须保留");
    assert!(
        dto.entries.iter().any(|entry| entry.from_cli),
        "至少应有一个变量被标记为 CLI 动态来源"
    );
}
