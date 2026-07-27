//! A2aAgentDal 单元测试
//!
//! 验证派生 Dal 的核心行为：
//! 1. 委托模式：调用方法时委托给 base
//! 2. prompt_builder 默认复用：未重写时走 trait 默认方法返回 DefaultPromptBuilder

use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dal::agent::{AgentDal, AgentFetchOptions};
use crate::service::dal::agent_a2a::A2aAgentDal;
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use std::sync::{Arc, Mutex};

/// Minimal mock AgentDal，记录方法调用
struct MockAgentDal {
    calls: Mutex<Vec<String>>,
}

impl MockAgentDal {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, name: &str) {
        self.calls.lock().unwrap().push(name.to_string());
    }

    fn was_called(&self, name: &str) -> bool {
        self.calls.lock().unwrap().iter().any(|c| c == name)
    }
}

#[async_trait::async_trait]
impl AgentDal for MockAgentDal {
    async fn create(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record("create");
        Ok(())
    }
    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<Agent>> {
        self.record("find_by_id");
        Ok(None)
    }
    async fn get_agent(
        &self,
        _ctx: RequestContext,
        _id: &str,
        _options: AgentFetchOptions,
    ) -> Result<Option<Agent>> {
        self.record("get_agent");
        Ok(None)
    }
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::agent::AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.record("query");
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn count(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::agent::AgentQuery,
    ) -> Result<u64> {
        self.record("count");
        Ok(0)
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<Agent>> {
        self.record("find_all");
        Ok(Vec::new())
    }
    async fn search(
        &self,
        _ctx: RequestContext,
        _search: crate::service::dao::agent::AgentSearch,
    ) -> Result<Vec<Agent>> {
        self.record("search");
        Ok(Vec::new())
    }
    async fn update(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record("update");
        Ok(())
    }
    async fn delete(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record("delete");
        Ok(())
    }
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &mut Agent,
        _brain: Brain,
    ) -> Result<()> {
        self.record("wake_brain");
        Ok(())
    }
    async fn get_stats(
        &self,
        _ctx: RequestContext,
        _agent_id: &str,
        _options: StatsFetchOptions,
    ) -> Result<AgentStats> {
        self.record("get_stats");
        Ok(AgentStats::default())
    }
    async fn get_model_call_stats(
        &self,
        _ctx: RequestContext,
        _agent_id: &str,
        _options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        self.record("get_model_call_stats");
        Ok(ModelCallStats::default())
    }
    async fn rebuild_vectors(&self, _ctx: RequestContext) -> Result<()> {
        self.record("rebuild_vectors");
        Ok(())
    }
    // prompt_builder 走 trait 默认方法
}

fn make_test_ctx() -> RequestContext {
    RequestContext::new(Some("tester".to_string()), None)
}

#[tokio::test]
async fn a2a_agent_dal_delegates_create_to_base() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mock = Arc::new(MockAgentDal::new());
    let a2a_dal = A2aAgentDal::new(mock.clone());

    let agent_po = AgentPo::new(
        "Remote Agent".to_string(),
        vec!["remote".to_string()],
        "A2A agent".to_string(),
        vec![],
        String::new(),
        String::new(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    let result = a2a_dal.create(make_test_ctx(), &agent).await;
    assert!(result.is_ok());
    assert!(mock.was_called("create"));
}

#[tokio::test]
async fn a2a_agent_dal_delegates_get_agent_to_base() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mock = Arc::new(MockAgentDal::new());
    let a2a_dal = A2aAgentDal::new(mock.clone());

    let result = a2a_dal
        .get_agent(make_test_ctx(), "agent-x", Default::default())
        .await;
    assert!(result.is_ok());
    assert!(mock.was_called("get_agent"));
}

/// 验证未重写 prompt_builder 时走 trait 默认方法返回 DefaultPromptBuilder
#[test]
fn a2a_agent_dal_default_prompt_builder_returns_default() {
    use crate::models::tool::ToolPo;
    use common::enums::ToolProtocol;
    use serde_json::json;

    let mock = Arc::new(MockAgentDal::new());
    let a2a_dal = A2aAgentDal::new(mock);

    let agent_po = AgentPo::new(
        "工具助手".to_string(),
        vec!["test".to_string()],
        "可以使用工具".to_string(),
        vec!["工具调用".to_string()],
        "按需使用工具。".to_string(),
        "provider-001".to_string(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    let tool_po = ToolPo::new(
        "remote-tool".to_string(),
        "remote-tool".to_string(),
        "A2A tool".to_string(),
        ToolProtocol::Mcp,
        json!({}),
        Some(json!({"type": "object"})),
        vec!["test".to_string()],
        Some("creator".to_string()),
    );

    let mut builder = a2a_dal.prompt_builder();
    builder.system_prompt(&agent);
    builder.tools(&[tool_po]);
    let prompt = builder.build();
    assert!(prompt.contains("【常用工具】"));
    assert!(prompt.contains("remote-tool"));
}
