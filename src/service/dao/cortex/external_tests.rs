//! tests 单元测试（拆分自 external.rs）
//!
//! 文件瘦身：原 297 行 → 131 行，测试体 167 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use common::enums::AgentKind;

fn make_cli_agent() -> AgentPo {
    let mut agent = AgentPo::new(
        "test-cli".to_string(),
        vec!["coder".to_string()],
        "test agent".to_string(),
        vec!["code".to_string()],
        "soul".to_string(),
        "".to_string(),
        "creator-1".to_string(),
    );
    agent.kind = AgentKind::Cli;
    agent.set_external_config(ExternalAgentConfig::Cli {
        command: "echo".to_string(),
        args: vec!["hello".to_string()],
        work_dir: "/tmp".to_string(),
        env: vec![],
        timeout_secs: 10,
        prompt_template: None,
    });
    agent
}

fn make_remote_agent() -> AgentPo {
    let mut agent = AgentPo::new(
        "test-remote".to_string(),
        vec!["helper".to_string()],
        "remote test agent".to_string(),
        vec!["chat".to_string()],
        "soul".to_string(),
        "".to_string(),
        "creator-1".to_string(),
    );
    agent.kind = AgentKind::Remote;
    agent.set_external_config(ExternalAgentConfig::Remote {
        endpoint: "http://example.com/a2a".to_string(),
        agent_name: "remote-bot".to_string(),
        auth_token: Some("token123".to_string()),
        timeout_secs: 30,
    });
    agent
}

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo {
        id: "external".to_string(),
        name: "External".to_string(),
        provider_type: common::enums::ProviderType::Custom,
        model_name: "external".to_string(),
        capability: common::enums::ModelCapability::Agent,
        api_key: "".to_string(),
        base_url: None,
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }
}

#[tokio::test]
async fn test_from_agent_cli() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = make_cli_agent();
    let cortex = ExternalCortexDao::from_agent(&agent);
    assert!(cortex.is_some());
    let cortex = cortex.unwrap();
    assert_eq!(cortex.agent_id(), agent.id);
    assert_eq!(cortex.agent_name(), "test-cli");
}

#[tokio::test]
async fn test_from_agent_remote() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = make_remote_agent();
    let cortex = ExternalCortexDao::from_agent(&agent);
    assert!(cortex.is_some());
    let cortex = cortex.unwrap();
    assert_eq!(cortex.agent_id(), agent.id);
    assert_eq!(cortex.agent_name(), "test-remote");
}

#[tokio::test]
async fn test_from_agent_local_returns_none() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = AgentPo::new(
        "local-bot".to_string(),
        vec!["worker".to_string()],
        "local".to_string(),
        vec!["chat".to_string()],
        "soul".to_string(),
        "provider-1".to_string(),
        "creator-1".to_string(),
    );
    assert_eq!(agent.kind, AgentKind::Local);
    let cortex = ExternalCortexDao::from_agent(&agent);
    assert!(cortex.is_none());
}

#[tokio::test]
async fn test_from_agent_cli_without_config_returns_none() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mut agent = AgentPo::new(
        "no-config".to_string(),
        vec!["worker".to_string()],
        "no config".to_string(),
        vec!["code".to_string()],
        "soul".to_string(),
        "".to_string(),
        "creator-1".to_string(),
    );
    agent.kind = AgentKind::Cli;
    let cortex = ExternalCortexDao::from_agent(&agent);
    assert!(cortex.is_none());
}

#[tokio::test]
async fn test_embed_not_supported() {
    crate::pkg::storage::test_support::init_for_test().await;
    let agent = make_cli_agent();
    let cortex = ExternalCortexDao::from_agent(&agent).unwrap();
    let ctx = RequestContext::new_system();
    let provider = mock_provider();
    let result = cortex.embed(ctx, &provider, &["hello".to_string()]).await;
    assert!(result.is_err());
    let e = result.unwrap_err();
    assert!(e.to_string().contains("不支持 embed"));
}

#[tokio::test]
async fn test_think_cli_cat() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mut agent = AgentPo::new(
        "cli-test".to_string(),
        vec!["coder".to_string()],
        "test".to_string(),
        vec!["code".to_string()],
        "soul".to_string(),
        "".to_string(),
        "creator-1".to_string(),
    );
    agent.kind = AgentKind::Cli;
    agent.set_external_config(ExternalAgentConfig::Cli {
        command: "cat".to_string(),
        args: vec![],
        work_dir: "/tmp".to_string(),
        env: vec![],
        timeout_secs: 10,
        prompt_template: None,
    });

    let cortex = ExternalCortexDao::from_agent(&agent).unwrap();
    let ctx = RequestContext::new_system();
    let provider = mock_provider();
    let result = cortex
        .think(ctx, &provider, &[ChatMessage::user("hello world")], &[])
        .await;
    assert!(result.is_ok(), "expected ok, got {:?}", result);
    match result.unwrap() {
        ThinkResult::Final { content, .. } => assert_eq!(content, "hello world"),
        _ => panic!("expected ThinkResult::Final"),
    }
}
