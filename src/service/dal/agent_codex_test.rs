//! CodexAgentDal 单元测试
//!
//! 验证派生 Dal 的核心行为：
//! 1. 委托模式：调用方法时委托给 base
//! 2. prompt_builder 默认复用：未重写时走 trait 默认方法返回 DefaultPromptBuilder

use crate::models::agent::{Agent, AgentPo};
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dal::agent::{AgentDal, AgentFetchOptions};
use crate::service::dal::agent_codex::CodexAgentDal;
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use std::sync::{Arc, Mutex};

/// Minimal mock AgentDal，记录方法调用，所有方法返回空结果或错误
struct MockAgentDal {
    /// 记录被调用的方法名
    calls: Mutex<Vec<String>>,
}

impl MockAgentDal {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record_call(&self, name: &str) {
        self.calls.lock().unwrap().push(name.to_string());
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn was_called(&self, name: &str) -> bool {
        self.calls.lock().unwrap().iter().any(|c| c == name)
    }
}

#[async_trait::async_trait]
impl AgentDal for MockAgentDal {
    async fn create(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record_call("create");
        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<Agent>> {
        self.record_call("find_by_id");
        Ok(None)
    }

    async fn get_agent(
        &self,
        _ctx: RequestContext,
        _id: &str,
        _options: AgentFetchOptions,
    ) -> Result<Option<Agent>> {
        self.record_call("get_agent");
        Ok(None)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::agent::AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.record_call("query");
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
        self.record_call("count");
        Ok(0)
    }

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<Agent>> {
        self.record_call("find_all");
        Ok(Vec::new())
    }

    async fn search(
        &self,
        _ctx: RequestContext,
        _search: crate::service::dao::agent::AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.record_call("search");
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }

    async fn update(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record_call("update");
        Ok(())
    }

    async fn delete(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
        self.record_call("delete");
        Ok(())
    }

    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &mut Agent,
        _brain: Brain,
    ) -> Result<()> {
        self.record_call("wake_brain");
        Ok(())
    }

    async fn get_stats(
        &self,
        _ctx: RequestContext,
        _agent_id: &str,
        _options: StatsFetchOptions,
    ) -> Result<AgentStats> {
        self.record_call("get_stats");
        Ok(AgentStats::default())
    }

    async fn get_model_call_stats(
        &self,
        _ctx: RequestContext,
        _agent_id: &str,
        _options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        self.record_call("get_model_call_stats");
        Ok(ModelCallStats::default())
    }

    async fn rebuild_vectors(&self, _ctx: RequestContext) -> Result<()> {
        self.record_call("rebuild_vectors");
        Ok(())
    }
    // prompt_builder 走 trait 默认方法，不重写
}

fn make_test_ctx() -> RequestContext {
    // Mock AgentDal 不使用 ctx 内容，构造一个 minimal ctx 即可
    RequestContext::new(Some("tester".to_string()), None)
}

#[tokio::test]
async fn codex_agent_dal_delegates_create_to_base() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mock = Arc::new(MockAgentDal::new());
    let codex_dal = CodexAgentDal::new(mock.clone());

    let agent_po = AgentPo::new(
        "Codex Agent".to_string(),
        vec!["coder".to_string()],
        "CLI agent".to_string(),
        vec![],
        String::new(),
        String::new(),
        "tester".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    let result = codex_dal.create(make_test_ctx(), &agent).await;
    assert!(result.is_ok());
    assert!(mock.was_called("create"));
    assert_eq!(mock.call_count(), 1);
}

#[tokio::test]
async fn codex_agent_dal_delegates_find_by_id_to_base() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mock = Arc::new(MockAgentDal::new());
    let codex_dal = CodexAgentDal::new(mock.clone());

    let result = codex_dal.find_by_id(make_test_ctx(), "agent-1").await;
    assert!(result.is_ok());
    assert!(mock.was_called("find_by_id"));
}

#[tokio::test]
async fn codex_agent_dal_delegates_rebuild_vectors_to_base() {
    crate::pkg::storage::test_support::init_for_test().await;
    let mock = Arc::new(MockAgentDal::new());
    let codex_dal = CodexAgentDal::new(mock.clone());

    let result = codex_dal.rebuild_vectors(make_test_ctx()).await;
    assert!(result.is_ok());
    assert!(mock.was_called("rebuild_vectors"));
}

/// 验证未重写 prompt_builder 时走 trait 默认方法返回 DefaultPromptBuilder
#[test]
fn codex_agent_dal_default_prompt_builder_returns_default() {
    let mock = Arc::new(MockAgentDal::new());
    let codex_dal = CodexAgentDal::new(mock);

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

    let mut builder = codex_dal.prompt_builder();
    builder.system_prompt(&agent);
    let prompt = builder.build();
    // 工具列表不再注入 Prompt（通过 OpenAI tools API 协议层传递）
    // 仅验证 builder 走默认实现，工具不出现
    assert!(prompt.contains("工具助手"));
    assert!(!prompt.contains("【常用工具】"));
    assert!(!prompt.contains("test-tool"));
}

/// 验证 prompt_builder 返回的 builder 可以多次调用 build()（&self 风格）
#[test]
fn codex_agent_dal_prompt_builder_supports_repeated_build() {
    let mock = Arc::new(MockAgentDal::new());
    let codex_dal = CodexAgentDal::new(mock);

    let mut builder = codex_dal.prompt_builder();
    builder.current_trace_id("trace-A");
    let prompt1 = builder.build();
    let prompt2 = builder.build();
    // &self 风格允许重复 build，结果一致
    assert_eq!(prompt1, prompt2);
    assert!(prompt1.contains("trace-A"));
}
