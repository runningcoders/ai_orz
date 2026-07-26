//! ExternalCortex - 外部 Agent 的虚拟 Cortex
//!
//! 为外部 Agent（CLI / A2A 远程）实现 `CortexTrait`，
//! 使其能装配到 Brain 中，复用现有的 think / awaken 链路。
//!
//! 设计：
//! - 实现 CortexTrait 的 6 个方法
//! - prompt() 桥接到 AgentRuntimeDao 执行
//! - embeddings() 不支持（外部 agent 自己处理向量）
//! - support_tools() 返回 false（第一版外部 agent 不支持工具调用）

use async_trait::async_trait;
use common::enums::ModelCapability;
use dyn_clone::clone_box;

use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::brain::CortexTrait;
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime::{
    AgentRuntimeDao, a2a::A2aRuntimeDao, codex::CodexRuntimeDao,
};

/// 外部 Agent 的虚拟 Cortex
pub struct ExternalCortex {
    agent: AgentPo,
    runtime_dao: Box<dyn AgentRuntimeDao>,
}

impl ExternalCortex {
    /// 从 AgentPo 创建 ExternalCortex
    ///
    /// 如果 Agent 不是外部类型或配置不完整，返回 None
    pub fn from_agent(agent: &AgentPo) -> Option<Self> {
        let config = agent.get_external_config()?;
        let runtime_dao: Box<dyn AgentRuntimeDao> = match config {
            ExternalAgentConfig::Cli {
                command,
                args,
                work_dir,
                env,
                timeout_secs,
                prompt_template,
            } => Box::new(CodexRuntimeDao::new(
                crate::service::dao::agent_runtime::codex::CliRuntimeConfig {
                    command,
                    args,
                    work_dir,
                    env,
                    timeout_secs,
                    prompt_template,
                },
            )),
            ExternalAgentConfig::Remote {
                endpoint,
                agent_name,
                auth_token,
                timeout_secs,
            } => Box::new(A2aRuntimeDao::new(
                crate::service::dao::agent_runtime::a2a::A2aRuntimeConfig {
                    endpoint,
                    agent_name,
                    auth_token,
                    timeout_secs,
                },
            )),
        };

        Some(Self {
            agent: agent.clone(),
            runtime_dao,
        })
    }

    /// 获取 Agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent.id
    }

    /// 获取 Agent 名称
    pub fn agent_name(&self) -> &str {
        &self.agent.name
    }
}

impl Clone for ExternalCortex {
    fn clone(&self) -> Self {
        Self {
            agent: self.agent.clone(),
            runtime_dao: clone_box(&*self.runtime_dao),
        }
    }
}

#[async_trait]
impl CortexTrait for ExternalCortex {
    fn capability(&self) -> ModelCapability {
        ModelCapability::Agent
    }

    fn model_provider_id(&self) -> &str {
        "external"
    }

    fn model_name(&self) -> &str {
        &self.agent.name
    }

    async fn prompt(&self, prompt: &str) -> anyhow::Result<String> {
        let ctx = RequestContext::new(None, None);
        self.runtime_dao
            .invoke(ctx, &self.agent, prompt)
            .await
            .map_err(|e| anyhow::anyhow!("external agent invoke failed: {}", e))
    }

    async fn embeddings(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Err(anyhow::anyhow!(
            "ExternalCortex does not support embeddings"
        ))
    }

    fn support_tools(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
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

    #[tokio::test]
    async fn test_from_agent_cli() {
        crate::pkg::storage::test_support::init_for_test().await;
        let agent = make_cli_agent();
        let cortex = ExternalCortex::from_agent(&agent);
        assert!(cortex.is_some());
        let cortex = cortex.unwrap();
        assert_eq!(cortex.agent_id(), agent.id);
        assert_eq!(cortex.agent_name(), "test-cli");
        assert_eq!(cortex.capability(), ModelCapability::Agent);
        assert_eq!(cortex.model_provider_id(), "external");
        assert_eq!(cortex.model_name(), "test-cli");
        assert!(!cortex.support_tools());
    }

    #[tokio::test]
    async fn test_from_agent_remote() {
        crate::pkg::storage::test_support::init_for_test().await;
        let agent = make_remote_agent();
        let cortex = ExternalCortex::from_agent(&agent);
        assert!(cortex.is_some());
        let cortex = cortex.unwrap();
        assert_eq!(cortex.agent_id(), agent.id);
        assert_eq!(cortex.agent_name(), "test-remote");
        assert_eq!(cortex.capability(), ModelCapability::Agent);
        assert_eq!(cortex.model_provider_id(), "external");
        assert_eq!(cortex.model_name(), "test-remote");
        assert!(!cortex.support_tools());
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
        let cortex = ExternalCortex::from_agent(&agent);
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
        let cortex = ExternalCortex::from_agent(&agent);
        assert!(cortex.is_none());
    }

    #[tokio::test]
    async fn test_embeddings_not_supported() {
        crate::pkg::storage::test_support::init_for_test().await;
        let agent = make_cli_agent();
        let cortex = ExternalCortex::from_agent(&agent).unwrap();
        let result = cortex.embeddings(&["hello".to_string()]).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not support embeddings"));
    }

    #[tokio::test]
    async fn test_prompt_cli_cat() {
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

        let cortex = ExternalCortex::from_agent(&agent).unwrap();
        let result = cortex.prompt("hello world").await;
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_clone() {
        crate::pkg::storage::test_support::init_for_test().await;
        let agent = make_cli_agent();
        let cortex = ExternalCortex::from_agent(&agent).unwrap();
        let cloned = cortex.clone();
        assert_eq!(cloned.agent_id(), cortex.agent_id());
        assert_eq!(cloned.agent_name(), cortex.agent_name());
        assert_eq!(cloned.model_name(), cortex.model_name());
    }
}
