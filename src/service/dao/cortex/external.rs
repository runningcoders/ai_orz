//! ExternalCortexDao - 外部 Agent 的虚拟 Cortex DAO
//!
//! 为外部 Agent（CLI / A2A 远程）实现 `native::CortexDao` trait，
//! 使其能通过统一接口被调用。
//!
//! 设计：
//! - `think()` 桥接到 AgentRuntimeDao 执行（包装为 ThinkResult::Final）
//! - `embed()` 不支持（外部 agent 自己处理向量），返回 Internal 错误

use async_trait::async_trait;
use common::error::{Result, err};

use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime::{
    AgentRuntimeDao, a2a::A2aRuntimeDao, codex::CodexRuntimeDao,
};
use crate::service::dao::cortex::native::CortexDao;

/// 外部 Agent 的虚拟 Cortex DAO
///
/// 注意：当前不通过 registry 路由（external agent 不走 cortex::native::registry()），
/// 而是由 brain.think() 直接按 brain.kind 分发。此实现保留供未来统一接入。
pub struct ExternalCortexDao {
    agent: AgentPo,
    runtime_dao: Box<dyn AgentRuntimeDao>,
}

impl ExternalCortexDao {
    /// 从 AgentPo 创建 ExternalCortexDao
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

#[async_trait]
impl CortexDao for ExternalCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        _provider: &ModelProviderPo,
        prompt: &str,
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        let content = self
            .runtime_dao
            .invoke(ctx, &self.agent, prompt)
            .await
            .map_err(|e| err!(Internal, "external agent invoke failed: {}", e))?;
        Ok(ThinkResult::Final {
            content,
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Err(err!(Internal, "ExternalCortexDao 不支持 embed"))
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
        let result = cortex.think(ctx, &provider, "hello world", &[]).await;
        assert!(result.is_ok(), "expected ok, got {:?}", result);
        match result.unwrap() {
            ThinkResult::Final { content, .. } => assert_eq!(content, "hello world"),
            _ => panic!("expected ThinkResult::Final"),
        }
    }
}
