//! Runtime ToolExecution protocol routing tests.

/// 凭据编排取数测试 stub（runtime 各测试模块共享注入，无需 DB）
///
/// 默认 `none()` 为未命中语义（既有测试不涉及凭据编排，语义不受影响）；
/// `with_*` 构造可配置命中值供 `resolve_tool_credentials` 测试使用。
#[cfg(test)]
pub(crate) mod credential_stubs {
    use crate::models::message_channel::MessageChannel;
    use crate::models::user::UserPo;
    use crate::models::user_credential::UserCredential;
    use crate::pkg::RequestContext;
    use crate::service::dal::lark::LarkCredentialDal;
    use crate::service::dal::user::UserDal;
    use crate::service::dao::lark::LarkAppCredentials;
    use crate::service::dao::user::UserQuery;
    use crate::service::dao::user_credential::UserCredentialQuery;
    use async_trait::async_trait;
    use common::api::PagedResult;
    use common::error::Result;
    use common::models::CredentialKind;

    /// UserDal stub：`find_default_credential` 可配置，其余方法与本模块测试无关
    pub(crate) struct StubUserDal {
        default_credential: Option<UserCredential>,
    }

    impl StubUserDal {
        /// 未命中语义（默认注入）
        pub(crate) fn none() -> Self {
            Self {
                default_credential: None,
            }
        }

        /// 命中指定默认凭证
        pub(crate) fn with_default(credential: UserCredential) -> Self {
            Self {
                default_credential: Some(credential),
            }
        }
    }

    #[async_trait]
    impl UserDal for StubUserDal {
        async fn create(&self, _ctx: RequestContext, _user: &UserPo) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<UserPo>> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_by_username(
            &self,
            _ctx: RequestContext,
            _username: &str,
        ) -> Result<Option<UserPo>> {
            unimplemented!("not needed by credential stub")
        }

        async fn query(
            &self,
            _ctx: RequestContext,
            _query: UserQuery,
        ) -> Result<PagedResult<UserPo>> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_by_organization_id(
            &self,
            _ctx: RequestContext,
            _org_id: &str,
        ) -> Result<Vec<UserPo>> {
            unimplemented!("not needed by credential stub")
        }

        async fn update(&self, _ctx: RequestContext, _user: &UserPo) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn delete(&self, _ctx: RequestContext, _id: &str) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn exists_by_username(&self, _ctx: RequestContext, _username: &str) -> Result<bool> {
            unimplemented!("not needed by credential stub")
        }

        async fn count_by_organization_id(
            &self,
            _ctx: RequestContext,
            _org_id: &str,
        ) -> Result<u64> {
            unimplemented!("not needed by credential stub")
        }

        async fn count(&self, _ctx: RequestContext, _query: UserQuery) -> Result<u64> {
            unimplemented!("not needed by credential stub")
        }

        async fn query_credentials(
            &self,
            _ctx: RequestContext,
            _query: UserCredentialQuery,
        ) -> Result<PagedResult<UserCredential>> {
            unimplemented!("not needed by credential stub")
        }

        async fn count_credentials(
            &self,
            _ctx: RequestContext,
            _query: UserCredentialQuery,
        ) -> Result<u64> {
            unimplemented!("not needed by credential stub")
        }

        async fn insert_credential(
            &self,
            _ctx: RequestContext,
            _credential: &UserCredential,
        ) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_credential_by_id(
            &self,
            _ctx: RequestContext,
            _id: &str,
        ) -> Result<Option<UserCredential>> {
            unimplemented!("not needed by credential stub")
        }

        async fn update_credential(
            &self,
            _ctx: RequestContext,
            _credential: &UserCredential,
        ) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn soft_delete_credential(&self, _ctx: RequestContext, _id: &str) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_default_credential(
            &self,
            _ctx: RequestContext,
            _user_id: &str,
            _kind: CredentialKind,
            _platform: Option<&str>,
        ) -> Result<Option<UserCredential>> {
            Ok(self.default_credential.clone())
        }

        async fn set_default_credential(
            &self,
            _ctx: RequestContext,
            _credential_id: &str,
        ) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }

        async fn clear_default_credential(
            &self,
            _ctx: RequestContext,
            _user_id: &str,
            _kind: CredentialKind,
            _platform: Option<&str>,
        ) -> Result<()> {
            unimplemented!("not needed by credential stub")
        }
    }

    /// LarkCredentialDal stub：`resolve_credentials_for_user` 可配置，其余方法与本模块测试无关
    pub(crate) struct StubLarkCredentialDal {
        credentials: Option<(LarkAppCredentials, String)>,
    }

    impl StubLarkCredentialDal {
        /// 未命中语义（默认注入）
        pub(crate) fn none() -> Self {
            Self { credentials: None }
        }

        /// 命中指定凭证 + 身份模式
        pub(crate) fn with_credentials(app_id: &str, app_secret: &str, mode: &str) -> Self {
            Self {
                credentials: Some((
                    LarkAppCredentials {
                        app_id: app_id.to_string(),
                        app_secret: app_secret.to_string(),
                    },
                    mode.to_string(),
                )),
            }
        }
    }

    #[async_trait]
    impl LarkCredentialDal for StubLarkCredentialDal {
        async fn resolve_credentials_for_user(
            &self,
            _ctx: &RequestContext,
        ) -> Result<Option<(LarkAppCredentials, String)>> {
            Ok(self.credentials.clone())
        }

        async fn resolve_channel_app_id(
            &self,
            _ctx: RequestContext,
            _channel: &MessageChannel,
        ) -> Option<String> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_channels_by_credential_id(
            &self,
            _credential_id: &str,
        ) -> Result<Vec<MessageChannel>> {
            unimplemented!("not needed by credential stub")
        }

        async fn find_channel_by_lark_identity(
            &self,
            _ctx: RequestContext,
            _app_id: &str,
            _open_id: &str,
        ) -> Result<Option<MessageChannel>> {
            unimplemented!("not needed by credential stub")
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::agent::{Agent, AgentPo};
    use crate::models::brain::Brain;
    use crate::models::memory::Memory;
    use crate::models::model_provider::ModelProvider;
    use crate::models::tool::{Tool, ToolCallTraceRef, ToolExecutionRequest, ToolPo};
    use crate::pkg::RequestContext;
    use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
    use crate::pkg::tool_tracing::logger::{ToolCallLogger, ToolCallQuery};
    use crate::service::dal::agent::{AgentDal, AgentFetchOptions};
    use crate::service::dal::brain::BrainDal;
    use crate::service::dal::mcp_tool::McpToolDal;
    use crate::service::dal::tool::ToolDal;
    use crate::service::dao::agent::{AgentQuery, AgentSearch};
    use crate::service::dao::tool::{ToolQuery, ToolSearch};
    use async_trait::async_trait;
    use common::enums::{CallerType, ControlMode, ToolProtocol, ToolStatus, UserRole};
    use common::error::Result;
    use common::models::{AgentStats, ModelCallStats, StatsFetchOptions, ToolStats};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;

    use super::credential_stubs::{StubLarkCredentialDal, StubUserDal};

    struct StubBrainDal;

    #[async_trait]
    impl BrainDal for StubBrainDal {
        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &AgentPo,
            _memories: Vec<Memory>,
        ) -> Result<Brain> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn test_connection(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _prompt: &str,
        ) -> Result<String> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn think(
            &self,
            _ctx: RequestContext,
            _brain: &Brain,
            _messages: &[crate::models::cortex_types::ChatMessage],
            _tools: &[crate::models::cortex_types::ToolDescriptor],
        ) -> Result<crate::models::cortex_types::ThinkResult> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn embed_entity(
            &self,
            _ctx: RequestContext,
            _entity: &dyn crate::models::vector::Vectorizable,
        ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
            Ok(None)
        }

        async fn embed_text_for_search(
            &self,
            _ctx: RequestContext,
            _text: &str,
        ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
            Ok(None)
        }
    }

    /// Stub AgentDal for tool execution tests.
    ///
    /// Returns a configurable Agent (or None) from find_by_id,
    /// so tests can verify installed_tags-based authorization.
    struct StubAgentDal {
        agent: Option<Agent>,
    }

    impl StubAgentDal {
        /// 返回一个默认 Agent（无 installed_tags），而非 None。
        ///
        /// 修复 Task 5.2: `call_manual_tool_for_agent` 中 agent 不存在时
        /// 现在返回错误而非 `unwrap_or_default()` 静默退化为空 vec。
        /// 此处返回带空 installed_tags 的 Agent，保留 "agent 存在但无安装包"
        /// 的语义，使相关 denial 测试能进入 installed_tags 检查路径。
        fn new() -> Self {
            Self {
                agent: Some(test_agent_with_installed_tags("test-agent", vec![])),
            }
        }

        fn with_agent(agent: Agent) -> Self {
            Self { agent: Some(agent) }
        }
    }

    #[async_trait]
    impl AgentDal for StubAgentDal {
        async fn create(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<Agent>> {
            Ok(self.agent.clone())
        }

        async fn get_agent(
            &self,
            _ctx: RequestContext,
            _id: &str,
            _options: AgentFetchOptions,
        ) -> Result<Option<Agent>> {
            Ok(self.agent.clone())
        }

        async fn query(
            &self,
            _ctx: RequestContext,
            _query: AgentQuery,
        ) -> Result<common::api::PagedResult<Agent>> {
            Ok(common::api::PagedResult {
                items: self.agent.iter().cloned().collect(),
                total: 0,
            })
        }

        async fn count(&self, _ctx: RequestContext, _query: AgentQuery) -> Result<u64> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<Agent>> {
            Ok(self.agent.iter().cloned().collect())
        }

        async fn search(
            &self,
            _ctx: RequestContext,
            _search: AgentSearch,
        ) -> Result<common::api::PagedResult<Agent>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn update(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn delete(&self, _ctx: RequestContext, _agent: &Agent) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &mut Agent,
            _brain: Brain,
        ) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_stats(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _options: StatsFetchOptions,
        ) -> Result<AgentStats> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_model_call_stats(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _options: StatsFetchOptions,
        ) -> Result<ModelCallStats> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn rebuild_vectors(&self, _ctx: RequestContext) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    struct RecordingToolDal {
        protocol: ToolProtocol,
        bound_tools: Vec<(String, ToolProtocol, ControlMode, ToolStatus)>,
        all_tools: Vec<ToolPo>,
        get_by_id_count: AtomicUsize,
        list_for_agent_count: AtomicUsize,
        query_count: AtomicUsize,
        call_by_id_count: AtomicUsize,
        call_tool_count: AtomicUsize,
    }

    impl RecordingToolDal {
        fn new(protocol: ToolProtocol) -> Self {
            Self {
                protocol,
                bound_tools: Vec::new(),
                all_tools: Vec::new(),
                get_by_id_count: AtomicUsize::new(0),
                list_for_agent_count: AtomicUsize::new(0),
                query_count: AtomicUsize::new(0),
                call_by_id_count: AtomicUsize::new(0),
                call_tool_count: AtomicUsize::new(0),
            }
        }

        fn with_bound_tools(bound_tools: Vec<(String, ToolProtocol, ControlMode)>) -> Self {
            let bound_tools = bound_tools
                .into_iter()
                .map(|(tool_id, protocol, control_mode)| {
                    (tool_id, protocol, control_mode, ToolStatus::Enabled)
                })
                .collect();
            Self::with_bound_tools_and_status(bound_tools)
        }

        fn with_bound_tools_and_status(
            bound_tools: Vec<(String, ToolProtocol, ControlMode, ToolStatus)>,
        ) -> Self {
            Self {
                protocol: ToolProtocol::Builtin,
                bound_tools,
                all_tools: Vec::new(),
                get_by_id_count: AtomicUsize::new(0),
                list_for_agent_count: AtomicUsize::new(0),
                query_count: AtomicUsize::new(0),
                call_by_id_count: AtomicUsize::new(0),
                call_tool_count: AtomicUsize::new(0),
            }
        }

        fn with_all_tools(mut self, all_tools: Vec<ToolPo>) -> Self {
            self.all_tools = all_tools;
            self
        }

        fn get_by_id_calls(&self) -> usize {
            self.get_by_id_count.load(Ordering::SeqCst)
        }

        fn list_for_agent_calls(&self) -> usize {
            self.list_for_agent_count.load(Ordering::SeqCst)
        }

        fn query_calls(&self) -> usize {
            self.query_count.load(Ordering::SeqCst)
        }

        fn call_by_id_calls(&self) -> usize {
            self.call_by_id_count.load(Ordering::SeqCst)
        }

        fn call_tool_calls(&self) -> usize {
            self.call_tool_count.load(Ordering::SeqCst)
        }

        fn tool(&self, tool_id: &str) -> Tool {
            Tool::from_po_for_management(test_tool_po(tool_id, self.protocol))
        }

        fn bound_tool(
            tool_id: &str,
            protocol: ToolProtocol,
            control_mode: ControlMode,
            status: ToolStatus,
        ) -> Tool {
            let mut po = test_tool_po_with_control_mode(tool_id, protocol, control_mode);
            po.status = status;
            Tool::from_po_for_management(po)
        }
    }

    #[async_trait]
    impl ToolDal for RecordingToolDal {
        async fn create_tool(&self, _ctx: RequestContext, _po: &ToolPo) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn update_tool(&self, _ctx: RequestContext, _tool: &Tool) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn delete_tool(&self, _ctx: RequestContext, _tool_id: &str) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_by_id(&self, _ctx: RequestContext, id: String) -> Result<Option<Tool>> {
            self.get_by_id_count.fetch_add(1, Ordering::SeqCst);
            Ok(Some(self.tool(&id)))
        }

        async fn get_tool(
            &self,
            ctx: RequestContext,
            id: &str,
            _options: crate::service::dal::tool::ToolFetchOptions,
        ) -> Result<Option<Tool>> {
            self.get_by_id(ctx, id.to_string()).await
        }

        async fn get_by_name(&self, _ctx: RequestContext, _name: &str) -> Result<Option<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn query(
            &self,
            _ctx: RequestContext,
            query: ToolQuery,
        ) -> Result<common::api::PagedResult<Tool>> {
            self.query_count.fetch_add(1, Ordering::SeqCst);
            let tools: Vec<Tool> = self
                .all_tools
                .iter()
                .map(|po| Tool::from_po_for_management(po.clone()))
                .collect();

            // 模拟 SQL 层 tag 过滤（OR 语义：命中任一 tag 即保留）
            let tools = if let Some(tags) = &query.tags {
                if tags.is_empty() {
                    tools
                } else {
                    tools
                        .into_iter()
                        .filter(|tool| {
                            let tool_tags = tool.po.get_tags();
                            tags.iter().any(|tag| tool_tags.contains(tag))
                        })
                        .collect()
                }
            } else {
                tools
            };

            Ok(common::api::PagedResult {
                items: tools,
                total: 0,
            })
        }

        async fn list_enabled(&self, _ctx: RequestContext) -> Result<Vec<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_tools_for_agent_full(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
        ) -> Result<Vec<Tool>> {
            self.list_for_agent_count.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .bound_tools
                .iter()
                .map(|(tool_id, protocol, control_mode, status)| {
                    Self::bound_tool(tool_id, *protocol, *control_mode, *status)
                })
                .collect())
        }

        async fn add_tool_to_agent(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _tool_id: &str,
            _created_by: Option<String>,
        ) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn remove_tool_from_agent(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _tool_id: &str,
        ) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn sync_builtin_tools_to_db(&self, _ctx: RequestContext) -> Result<usize> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_tool(
            &self,
            _ctx: RequestContext,
            request: ToolExecutionRequest,
        ) -> Result<(Value, ToolCallEntry)> {
            self.call_tool_count.fetch_add(1, Ordering::SeqCst);
            let entry = ToolCallEntry {
                tool_id: request.tool.id.clone(),
                call_id: "test-call-id".to_string(),
                ..Default::default()
            };
            Ok((
                json!({ "called_by": "tool_dal", "tool_id": request.tool.id, "args": request.args }),
                entry,
            ))
        }

        async fn search(
            &self,
            _ctx: RequestContext,
            _params: ToolSearch,
        ) -> Result<common::api::PagedResult<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_stats(
            &self,
            _ctx: RequestContext,
            _tool_id: &str,
            _options: StatsFetchOptions,
        ) -> Result<ToolStats> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn rebuild_vectors(&self, _ctx: RequestContext) -> Result<()> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_tags(&self, _ctx: RequestContext) -> Result<Vec<String>> {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    struct RecordingMcpToolDal {
        call_by_id_count: AtomicUsize,
        call_tool_count: AtomicUsize,
        error_message: Option<String>,
        error_trace_ref: Option<ToolCallTraceRef>,
    }

    impl RecordingMcpToolDal {
        fn new() -> Self {
            Self {
                call_by_id_count: AtomicUsize::new(0),
                call_tool_count: AtomicUsize::new(0),
                error_message: None,
                error_trace_ref: None,
            }
        }

        fn failing(error_message: &str) -> Self {
            Self {
                call_by_id_count: AtomicUsize::new(0),
                call_tool_count: AtomicUsize::new(0),
                error_message: Some(error_message.to_string()),
                error_trace_ref: None,
            }
        }

        fn failing_with_trace(error_message: &str, tool_id: &str, call_id: &str) -> Self {
            Self {
                call_by_id_count: AtomicUsize::new(0),
                call_tool_count: AtomicUsize::new(0),
                error_message: Some(error_message.to_string()),
                error_trace_ref: Some(ToolCallTraceRef {
                    tool_id: tool_id.to_string(),
                    call_id: call_id.to_string(),
                }),
            }
        }

        fn call_by_id_calls(&self) -> usize {
            self.call_by_id_count.load(Ordering::SeqCst)
        }

        fn call_tool_calls(&self) -> usize {
            self.call_tool_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl McpToolDal for RecordingMcpToolDal {
        async fn get_by_id(&self, _ctx: RequestContext, _tool_id: String) -> Result<Option<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn sync_from_server(&self, _ctx: RequestContext, _server_id: &str) -> Result<usize> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_by_server(
            &self,
            _ctx: RequestContext,
            _params: common::api::ListMcpToolsByServerRequest,
        ) -> Result<common::api::PagedResult<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_tool(
            &self,
            _ctx: RequestContext,
            request: ToolExecutionRequest,
        ) -> Result<(Value, ToolCallEntry)> {
            self.call_tool_count.fetch_add(1, Ordering::SeqCst);
            if let Some(error_message) = &self.error_message {
                return Err(match &self.error_trace_ref {
                    Some(trace_ref) => {
                        use common::error::{ErrorCode, ErrorType};
                        let mut err = common::error::Error::typed(
                            ErrorCode::ToolExecutionFailed,
                            ErrorType::Tool,
                            error_message.clone(),
                        );
                        let mut field = common::error::ErrorField::new();
                        field.insert(
                            "trace_ref".to_string(),
                            serde_json::to_value(trace_ref.clone()).unwrap_or_default(),
                        );
                        err = err.with_field(field);
                        err
                    }
                    None => common::error::Error::tool_call_failed(error_message.clone()),
                });
            }
            Ok((
                json!({ "called_by": "mcp_tool_dal", "tool_id": request.tool.id, "args": request.args }),
                ToolCallEntry {
                    tool_id: request.tool.id.clone(),
                    call_id: "test-call-id".to_string(),
                    ..Default::default()
                },
            ))
        }

        fn invalidate_server(&self, _server_id: &str) {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    /// Web 端用户请求（caller_type = User，ctx 无 agent/project/task 作用域）
    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    /// Agent 运行时请求（caller_type = Agent）——作用域 fail-closed 规则的适用对象
    fn agent_ctx() -> RequestContext {
        test_ctx()
            .to_builder()
            .caller_type(CallerType::Agent)
            .build()
    }

    fn test_runtime_with_tool_dals(
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    ) -> (
        tempfile::TempDir,
        Arc<dyn crate::service::domain::runtime::RuntimeDomain>,
    ) {
        test_runtime_with_all(tool_dal, mcp_tool_dal, Arc::new(StubAgentDal::new()))
    }

    fn test_runtime_with_all(
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        agent_dal: Arc<dyn AgentDal>,
    ) -> (
        tempfile::TempDir,
        Arc<dyn crate::service::domain::runtime::RuntimeDomain>,
    ) {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            tool_dal,
            mcp_tool_dal,
            agent_dal,
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );
        (temp_dir, runtime)
    }

    /// Agent 运行时带作用域的请求（scope fail-closed 测试的主力 ctx）
    fn scoped_test_ctx(agent_id: &str, project_id: &str, task_id: &str) -> RequestContext {
        test_ctx()
            .to_builder()
            .caller_type(CallerType::Agent)
            .agent_id(agent_id)
            .project_id(project_id)
            .task_id(task_id)
            .build()
    }

    fn test_tool_call_entry(
        call_id: &str,
        tool_id: &str,
        agent_id: &str,
        project_id: &str,
    ) -> ToolCallEntry {
        ToolCallEntry {
            call_id: call_id.to_string(),
            tool_id: tool_id.to_string(),
            tool_name: tool_id.to_string(),
            agent_id: Some(agent_id.to_string()),
            task_id: Some("runtime-task-1".to_string()),
            project_id: Some(project_id.to_string()),
            started_at: 1_760_000_000_000,
            finished_at: 1_760_000_000_100,
            duration_ms: 100,
            input: json!({"q": "weather"}),
            output: Some(json!({"ok": true})),
            error: None,
            status: ToolCallStatus::Completed,
            metadata: json!({"source": "runtime-test"}),
        }
    }

    fn test_tool_po(tool_id: &str, protocol: ToolProtocol) -> ToolPo {
        test_tool_po_with_control_mode(tool_id, protocol, ControlMode::Manual)
    }

    fn test_tool_po_with_control_mode(
        tool_id: &str,
        protocol: ToolProtocol,
        control_mode: ControlMode,
    ) -> ToolPo {
        let mut po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "test tool".to_string(),
            protocol,
            json!({}),
            Some(json!({ "type": "object" })),
            vec!["test".to_string()],
            Some("test-user".to_string()),
        );
        po.control_mode = control_mode;
        po
    }

    fn test_tool_po_with_tags(
        tool_id: &str,
        protocol: ToolProtocol,
        control_mode: ControlMode,
        tags: Vec<&str>,
    ) -> ToolPo {
        let mut po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "test tool".to_string(),
            protocol,
            json!({}),
            Some(json!({ "type": "object" })),
            tags.into_iter().map(String::from).collect(),
            Some("test-user".to_string()),
        );
        po.control_mode = control_mode;
        po
    }

    fn test_agent_with_installed_tags(agent_id: &str, tags: Vec<&str>) -> Agent {
        let mut po = crate::models::agent::AgentPo::new(
            "test-agent".to_string(),
            vec!["worker".to_string()],
            "test agent".to_string(),
            vec![],
            "test soul".to_string(),
            "test-provider".to_string(),
            "test-user".to_string(),
        );
        po.id = agent_id.to_string();
        for tag in tags {
            po.install_tag(tag);
        }
        Agent::from_po(po)
    }

    #[tokio::test]
    async fn runtime_query_tool_call_entries_derives_scope_from_context() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger.clone(),
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );
        let call_id = format!("runtime-query-{}", uuid::Uuid::now_v7());
        let entry = test_tool_call_entry(
            &call_id,
            "runtime-query-tool",
            "runtime-agent-1",
            "runtime-project-1",
        );
        logger
            .log_call("runtime-query-tool", entry.clone())
            .expect("trace entry should be logged");

        let results = runtime
            .tool_execution()
            .query_tool_call_entries(
                scoped_test_ctx("runtime-agent-1", "runtime-project-1", "runtime-task-1"),
                ToolCallQuery {
                    call_id: Some(call_id.clone()),
                    limit: Some(10),
                    ..Default::default()
                },
            )
            .await
            .expect("runtime query should succeed with context scope");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].call_id, call_id);
    }

    #[tokio::test]
    async fn runtime_tool_call_query_requires_access_scope() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        // Agent 调用：ctx 无任何作用域 → fail-closed
        let error = runtime
            .tool_execution()
            .query_tool_call_entries(agent_ctx(), ToolCallQuery::default())
            .await
            .expect_err("unscoped tool call query must fail closed");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_tool_call_query_requires_user_supplied_scope() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        // Web 用户（普通成员）无过滤条件 → 拒绝（禁止无边界遍历 trace）
        let error = runtime
            .tool_execution()
            .query_tool_call_entries(test_ctx(), ToolCallQuery::default())
            .await
            .expect_err("user query without any scope filter must be rejected");
        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);

        // Web 用户显式指定 agent 作用域 → 允许（Web 端 ctx 天然无作用域，只能靠查询条件收敛）
        let result = runtime
            .tool_execution()
            .query_tool_call_entries(
                test_ctx(),
                ToolCallQuery {
                    agent_id: Some("user-supplied-agent".to_string()),
                    ..Default::default()
                },
            )
            .await;
        assert!(result.is_ok(), "user query with explicit scope must pass");

        // Admin 用户：允许全量查询（可观测性管理页）
        let admin_ctx = test_ctx()
            .to_builder()
            .user_role(UserRole::Admin as i32)
            .build();
        let result = runtime
            .tool_execution()
            .query_tool_call_entries(admin_ctx, ToolCallQuery::default())
            .await;
        assert!(result.is_ok(), "admin unscoped query must pass");
    }

    #[tokio::test]
    async fn runtime_tool_call_query_rejects_request_scope_without_context_scope() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(
                agent_ctx(),
                ToolCallQuery {
                    agent_id: Some("user-supplied-agent".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("request-supplied scope without context scope must fail closed");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_tool_call_query_rejects_request_scope_without_matching_context_field() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );
        let mut ctx = agent_ctx();
        ctx.agent_id = Some("runtime-agent-only".to_string());

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(
                ctx,
                ToolCallQuery {
                    project_id: Some("user-supplied-project".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect_err(
                "request-supplied project scope without matching context project must fail closed",
            );

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_get_tool_call_entry_by_id_requires_call_id() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let error = runtime
            .tool_execution()
            .get_tool_call_entry_by_id(
                scoped_test_ctx(
                    "runtime-agent-call-id",
                    "runtime-project-call-id",
                    "runtime-task-1",
                ),
                ToolCallQuery::default(),
            )
            .await
            .expect_err("detail lookup without call_id must fail closed");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_tool_call_query_rejects_over_limit_request() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(
                scoped_test_ctx(
                    "runtime-agent-limit",
                    "runtime-project-limit",
                    "runtime-task-1",
                ),
                ToolCallQuery {
                    limit: Some(crate::pkg::tool_tracing::logger::MAX_TOOL_CALL_QUERY_LIMIT + 1),
                    ..Default::default()
                },
            )
            .await
            .expect_err("over-limit query must fail closed");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_tool_call_query_rejects_scope_conflicting_with_request_context() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger,
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(
                scoped_test_ctx("runtime-agent-3", "runtime-project-3", "runtime-task-1"),
                ToolCallQuery {
                    project_id: Some("other-project".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("query must not widen or swap scoped context");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
    }

    #[tokio::test]
    async fn runtime_get_tool_call_entry_by_id_filters_by_request_scope() {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            logger.clone(),
            Arc::new(StubUserDal::none()),
            Arc::new(StubLarkCredentialDal::none()),
        );
        let call_id = format!("runtime-get-{}", uuid::Uuid::now_v7());
        let entry = test_tool_call_entry(
            &call_id,
            "runtime-get-tool",
            "runtime-agent-2",
            "runtime-project-2",
        );
        logger
            .log_call("runtime-get-tool", entry)
            .expect("trace entry should be logged");

        let mismatched = runtime
            .tool_execution()
            .get_tool_call_entry_by_id(
                scoped_test_ctx("runtime-agent-2", "wrong-project", "runtime-task-1"),
                ToolCallQuery {
                    call_id: Some(call_id.clone()),
                    tool_id: Some("runtime-get-tool".to_string()),
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .expect("scoped query should succeed");
        assert!(mismatched.is_none());

        let matched = runtime
            .tool_execution()
            .get_tool_call_entry_by_id(
                scoped_test_ctx("runtime-agent-2", "runtime-project-2", "runtime-task-1"),
                ToolCallQuery {
                    call_id: Some(call_id.clone()),
                    tool_id: Some("runtime-get-tool".to_string()),
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .expect("scoped query should succeed");
        assert_eq!(matched.expect("entry should match scope").call_id, call_id);
    }

    #[tokio::test]
    async fn runtime_routes_mcp_tool_calls_to_mcp_tool_dal() {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let result = runtime
            .tool_execution()
            .call_tool_by_id(
                test_ctx(),
                "mcp-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .unwrap();

        assert_eq!(result.result["called_by"], "mcp_tool_dal");
        assert_eq!(tool_dal.get_by_id_calls(), 1);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 1);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_routes_already_loaded_mcp_tool_without_second_tool_lookup() {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());
        let tool = Tool::from_po_for_management(test_tool_po("mcp-tool-1", ToolProtocol::Mcp));

        let result = runtime
            .tool_execution()
            .call_tool(test_ctx(), &tool, json!({ "text": "hi" }))
            .await
            .unwrap();

        assert_eq!(result.result["called_by"], "mcp_tool_dal");
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 1);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_denies_manual_tool_call_when_tool_is_not_bound_to_agent() {
        let tool_dal = Arc::new(RecordingToolDal::with_bound_tools(vec![]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("unbound tool must be denied before protocol routing");

        let message = error.to_string();
        assert!(message.contains("Manual tool call denied"));
        assert!(message.contains("tool-1"));
        assert!(message.contains("agent-1"));
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.query_calls(), 1);
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 0);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_denies_manual_tool_call_when_bound_tool_is_auto_mode() {
        let tool_dal = Arc::new(RecordingToolDal::with_bound_tools(vec![(
            "tool-1".to_string(),
            ToolProtocol::Builtin,
            ControlMode::Auto,
        )]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("Auto tools must not execute through message-mode manual calls");

        let message = error.to_string();
        assert!(message.contains("Manual tool call denied"));
        assert!(message.contains("tool-1"));
        assert!(message.contains("Auto"));
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 0);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_denies_manual_tool_call_when_bound_tool_is_stale() {
        let tool_dal = Arc::new(RecordingToolDal::with_bound_tools_and_status(vec![(
            "tool-1".to_string(),
            ToolProtocol::Mcp,
            ControlMode::Manual,
            ToolStatus::Stale,
        )]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("Stale tools must not execute even when bindings remain");

        let message = error.to_string();
        assert!(message.contains("Tool execution denied"));
        assert!(message.contains("tool-1"));
        assert!(message.contains("Stale"));
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 0);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_executes_bound_manual_mcp_tool_without_second_tool_lookup() {
        let tool_dal = Arc::new(RecordingToolDal::with_bound_tools(vec![(
            "mcp-tool-1".to_string(),
            ToolProtocol::Mcp,
            ControlMode::Manual,
        )]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let result = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "mcp-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .unwrap();

        assert_eq!(result.result["called_by"], "mcp_tool_dal");
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 1);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_executes_bound_manual_builtin_tool_through_generic_tool_dal() {
        let tool_dal = Arc::new(RecordingToolDal::with_bound_tools(vec![(
            "builtin-tool-1".to_string(),
            ToolProtocol::Builtin,
            ControlMode::Manual,
        )]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let result = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "builtin-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .unwrap();

        assert_eq!(result.result["called_by"], "tool_dal");
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.get_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 1);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 0);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_routes_builtin_tool_calls_to_generic_tool_dal() {
        assert_non_mcp_protocol_routes_to_generic_tool_dal(ToolProtocol::Builtin, "builtin-tool-1")
            .await;
    }

    #[tokio::test]
    async fn runtime_routes_http_tool_calls_to_generic_tool_dal() {
        assert_non_mcp_protocol_routes_to_generic_tool_dal(ToolProtocol::Http, "http-tool-1").await;
    }

    #[tokio::test]
    async fn runtime_redacts_mcp_lower_layer_error_details() {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let sensitive_error = "failed to spawn command /opt/private/mcp-server with env API_TOKEN=placeholder-value and url https://example.invalid/mcp?credential=placeholder-value";
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::failing(sensitive_error));
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_tool_by_id(
                test_ctx(),
                "mcp-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("MCP lower-layer failure should be redacted at Runtime boundary");

        let message = error.to_string();
        assert!(message.contains("MCP tool call failed"));
        assert!(message.contains("mcp-tool-1"));
        assert!(!message.contains("/opt/private/mcp-server"));
        assert!(!message.contains("API_TOKEN"));
        assert!(!message.contains("placeholder-value"));
        assert!(!message.contains("credential"));
        assert_eq!(mcp_tool_dal.call_tool_calls(), 1);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_preserves_safe_mcp_timeout_error_semantics() {
        assert_mcp_lower_error_maps_to_safe_message(
            "MCP tool echo on server server-a timed out after 25ms",
            "MCP tool call timed out for tool_id: mcp-tool-1",
        )
        .await;
    }

    #[tokio::test]
    async fn runtime_preserves_safe_mcp_server_not_found_error_semantics() {
        assert_mcp_lower_error_maps_to_safe_message(
            "MCP server not found for tool mcp-tool-1: server-a",
            "MCP server not found for tool_id: mcp-tool-1",
        )
        .await;
    }

    #[tokio::test]
    async fn runtime_preserves_safe_mcp_server_disabled_error_semantics() {
        assert_mcp_lower_error_maps_to_safe_message(
            "MCP server disabled: server-a",
            "MCP server disabled for tool_id: mcp-tool-1",
        )
        .await;
    }

    #[tokio::test]
    async fn runtime_preserves_safe_mcp_tool_disabled_error_semantics() {
        assert_mcp_lower_error_maps_to_safe_message(
            "MCP tool disabled: mcp-tool-1",
            "MCP tool disabled: mcp-tool-1",
        )
        .await;
    }

    #[tokio::test]
    async fn runtime_preserves_safe_mcp_tool_not_found_error_semantics() {
        assert_mcp_lower_error_maps_to_safe_message(
            "Tool not found: mcp-tool-1 with config credential=placeholder-value",
            "MCP tool not found: mcp-tool-1",
        )
        .await;
    }

    #[tokio::test]
    async fn runtime_preserves_trace_ref_when_mcp_started_failure_is_mapped() {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::failing_with_trace(
            "MCP tool call failed for tool_id: mcp-tool-1",
            "mcp-tool-1",
            "real-mcp-call-777",
        ));
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_tool_by_id(
                test_ctx(),
                "mcp-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("started MCP failure should return structured execution error");

        // TODO: trace_ref is no longer on Error, it's stored separately
        // assert_eq!(
        //     error.trace_ref,
        //     Some(ToolCallTraceRef {
        //         tool_id: "mcp-tool-1".to_string(),
        //         call_id: "real-mcp-call-777".to_string(),
        //     })
        // );
        assert!(
            error
                .to_string()
                .contains("MCP tool call failed for tool_id: mcp-tool-1")
        );
    }

    async fn assert_non_mcp_protocol_routes_to_generic_tool_dal(
        protocol: ToolProtocol,
        tool_id: &str,
    ) {
        let tool_dal = Arc::new(RecordingToolDal::new(protocol));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let result = runtime
            .tool_execution()
            .call_tool_by_id(test_ctx(), tool_id.to_string(), json!({ "text": "hi" }))
            .await
            .unwrap();

        assert_eq!(result.result["called_by"], "tool_dal");
        assert_eq!(tool_dal.get_by_id_calls(), 1);
        assert_eq!(tool_dal.call_tool_calls(), 1);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
        assert_eq!(mcp_tool_dal.call_tool_calls(), 0);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
    }

    async fn assert_mcp_lower_error_maps_to_safe_message(lower_error: &str, expected: &str) {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::failing(lower_error));
        let (_temp_dir, runtime) =
            test_runtime_with_tool_dals(tool_dal.clone(), mcp_tool_dal.clone());

        let error = runtime
            .tool_execution()
            .call_tool_by_id(
                test_ctx(),
                "mcp-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("MCP semantic lower-layer failure should stay visible safely");

        let message = error.to_string();
        assert!(
            message.contains(expected),
            "message `{}` should contain `{}`",
            message,
            expected
        );
        assert!(!message.contains("placeholder-value"));
        assert!(!message.contains("API_TOKEN"));
        assert!(!message.contains("credential"));
        assert_eq!(mcp_tool_dal.call_tool_calls(), 1);
        assert_eq!(mcp_tool_dal.call_by_id_calls(), 0);
        assert_eq!(tool_dal.call_tool_calls(), 0);
        assert_eq!(tool_dal.call_by_id_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_allows_manual_tool_call_when_tool_pack_tag_is_installed() {
        // Tool with "project_management" tag, not bound to agent
        let tool_po = test_tool_po_with_tags(
            "pm-tool-1",
            ToolProtocol::Builtin,
            ControlMode::Manual,
            vec!["project_management"],
        );
        let tool_dal =
            Arc::new(RecordingToolDal::with_bound_tools(vec![]).with_all_tools(vec![tool_po]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let agent_dal = Arc::new(StubAgentDal::with_agent(test_agent_with_installed_tags(
            "agent-1",
            vec!["project_management"],
        )));
        let (_temp_dir, runtime) =
            test_runtime_with_all(tool_dal.clone(), mcp_tool_dal.clone(), agent_dal);

        let result = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "pm-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect("tool with installed tag should be allowed");

        assert_eq!(result.result["called_by"], "tool_dal");
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.query_calls(), 1);
        assert_eq!(tool_dal.call_tool_calls(), 1);
    }

    #[tokio::test]
    async fn runtime_denies_manual_tool_call_when_tool_pack_tag_not_installed() {
        // Tool with "data_analysis" tag, but agent only has "project_management"
        let tool_po = test_tool_po_with_tags(
            "da-tool-1",
            ToolProtocol::Builtin,
            ControlMode::Manual,
            vec!["data_analysis"],
        );
        let tool_dal =
            Arc::new(RecordingToolDal::with_bound_tools(vec![]).with_all_tools(vec![tool_po]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let agent_dal = Arc::new(StubAgentDal::with_agent(test_agent_with_installed_tags(
            "agent-1",
            vec!["project_management"],
        )));
        let (_temp_dir, runtime) =
            test_runtime_with_all(tool_dal.clone(), mcp_tool_dal.clone(), agent_dal);

        let error = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "da-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("tool with non-installed tag should be denied");

        let message = error.to_string();
        assert!(message.contains("Manual tool call denied"));
        assert!(message.contains("da-tool-1"));
        assert!(message.contains("agent-1"));
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.query_calls(), 1);
        assert_eq!(tool_dal.call_tool_calls(), 0);
    }

    #[tokio::test]
    async fn runtime_denies_manual_tool_call_when_agent_has_no_installed_tags() {
        // Tool with "project_management" tag, but agent has no installed_tags
        let tool_po = test_tool_po_with_tags(
            "pm-tool-1",
            ToolProtocol::Builtin,
            ControlMode::Manual,
            vec!["project_management"],
        );
        let tool_dal =
            Arc::new(RecordingToolDal::with_bound_tools(vec![]).with_all_tools(vec![tool_po]));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        // StubAgentDal::new() 返回带空 installed_tags 的 Agent
        let agent_dal = Arc::new(StubAgentDal::new());
        let (_temp_dir, runtime) =
            test_runtime_with_all(tool_dal.clone(), mcp_tool_dal.clone(), agent_dal);

        let error = runtime
            .tool_execution()
            .call_manual_tool_for_agent(
                test_ctx(),
                "agent-1".to_string(),
                "pm-tool-1".to_string(),
                json!({ "text": "hi" }),
            )
            .await
            .expect_err("tool should be denied when agent has no installed tags");

        let message = error.to_string();
        assert!(message.contains("Manual tool call denied"));
        assert!(message.contains("pm-tool-1"));
        assert!(message.contains("no installed tool packs"));
        assert_eq!(tool_dal.list_for_agent_calls(), 1);
        assert_eq!(tool_dal.query_calls(), 1);
        assert_eq!(tool_dal.call_tool_calls(), 0);
    }

    // ==================== resolve_tool_credentials（D17 生产路由二元化） ====================

    use crate::models::user_credential::{UserCredential, UserCredentialPo};
    use crate::service::domain::runtime::RuntimeDomainImpl;
    use common::models::{
        CredentialBinding, CredentialDetail, CredentialKind, CredentialRequirement,
        CredentialVisibility,
    };

    fn credential_requirement(
        kind: CredentialKind,
        field: Option<&str>,
        platform: Option<&str>,
    ) -> CredentialRequirement {
        CredentialRequirement {
            kind,
            platform: platform.map(String::from),
            field: field.map(String::from),
            enhancer: None,
            binding: CredentialBinding::Internal {
                field: "credential".to_string(),
            },
        }
    }

    /// 构造带凭据 stub 的具体类型实例（私有方法 resolve_tool_credentials 需具体类型调用）
    fn credential_runtime(
        user_dal: StubUserDal,
        lark_credentials: StubLarkCredentialDal,
    ) -> RuntimeDomainImpl {
        let temp_dir = tempdir().expect("tempdir should be created");
        RuntimeDomainImpl::new_with_all(
            Arc::new(StubBrainDal),
            Arc::new(RecordingToolDal::new(ToolProtocol::Builtin)),
            Arc::new(RecordingMcpToolDal::new()),
            Arc::new(StubAgentDal::new()),
            Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
            Arc::new(user_dal),
            Arc::new(lark_credentials),
        )
    }

    /// LarkApp 生产路由：走 lark 凭据子 trait，attributes 附 identity_mode（D24）
    #[tokio::test]
    async fn resolve_tool_credentials_lark_route_sets_identity_mode_attribute() {
        let runtime = credential_runtime(
            StubUserDal::none(),
            StubLarkCredentialDal::with_credentials("cli_a", "sec", "user"),
        );
        let requirements = vec![credential_requirement(
            CredentialKind::LarkApp,
            Some("identity_mode"),
            None,
        )];

        let resolved = runtime
            .resolve_tool_credentials(&test_ctx(), &requirements)
            .await
            .unwrap()
            .expect("lark route should hit");

        assert_eq!(resolved.len(), 1);
        // attributes 查找链命中派生属性（identity_mode 由生产端注入，D24）
        assert_eq!(resolved[0].value, "user");
    }

    /// user dal 生产路由：find_default_credential 命中 → 加密态经 pkg 解密取注入值
    #[tokio::test]
    async fn resolve_tool_credentials_user_dal_route_hits_default_credential() {
        let credential = UserCredential::from_po(UserCredentialPo::new(
            "cred-gh-1".to_string(),
            "org-1".to_string(),
            "test-user".to_string(),
            CredentialKind::GithubToken,
            "gh default".to_string(),
            CredentialDetail::GithubToken {
                token: "ghp_plain".to_string(),
            },
            CredentialVisibility::Private,
            "test-user".to_string(),
        ));
        let runtime = credential_runtime(
            StubUserDal::with_default(credential),
            StubLarkCredentialDal::none(),
        );
        let requirements = vec![credential_requirement(
            CredentialKind::GithubToken,
            None,
            None,
        )];

        let resolved = runtime
            .resolve_tool_credentials(&test_ctx(), &requirements)
            .await
            .unwrap()
            .expect("user dal route should hit");

        assert_eq!(resolved.len(), 1);
        // already_decrypted=false → pkg 解密路径（明文兼容：无 enc:v1: 前缀原样返回）
        assert_eq!(resolved[0].value, "ghp_plain");
    }

    /// 任一 requirement 未命中 → Ok(None)（调用方出引导）
    #[tokio::test]
    async fn resolve_tool_credentials_returns_none_when_any_requirement_misses() {
        let runtime = credential_runtime(
            StubUserDal::none(), // GithubToken 未命中
            StubLarkCredentialDal::with_credentials("cli_a", "sec", "auto"),
        );
        let requirements = vec![
            credential_requirement(CredentialKind::LarkApp, Some("identity_mode"), None),
            credential_requirement(CredentialKind::GithubToken, None, None),
        ];

        let resolved = runtime
            .resolve_tool_credentials(&test_ctx(), &requirements)
            .await
            .unwrap();

        assert!(resolved.is_none());
    }

    /// 空 requirements → Ok(Some(empty))（无凭据需求直通）
    #[tokio::test]
    async fn resolve_tool_credentials_empty_requirements_returns_some_empty() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());

        let resolved = runtime
            .resolve_tool_credentials(&test_ctx(), &[])
            .await
            .unwrap();

        assert!(resolved.is_some());
        assert!(resolved.unwrap().is_empty());
    }

    // ==================== tool_readiness 数据驱动判定（D28） ====================

    use crate::pkg::tool_registry::browser::BrowserToolFactory;
    use crate::pkg::tool_registry::get_registry;
    use crate::pkg::tool_registry::tavily_search::TavilySearchToolFactory;
    use crate::service::domain::runtime::RuntimeToolExecution;
    use crate::service::domain::runtime::tool_execution::{
        expire_readiness_cache, invalidate_readiness_cache,
    };
    use common::api::RuntimeReady;

    /// CLI 型 PO（config 带 command，可选 install_hint）
    fn cli_readiness_tool(tool_id: &str, command: &str, install_hint: Option<&str>) -> Tool {
        let mut config = json!({ "command": command });
        if let Some(hint) = install_hint {
            config["install_hint"] = json!(hint);
        }
        let mut po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "cli readiness tool".to_string(),
            ToolProtocol::Builtin,
            config,
            Some(json!({ "type": "object" })),
            vec!["test".to_string()],
            Some("test-user".to_string()),
        );
        po.control_mode = ControlMode::Manual;
        Tool::from_po_for_management(po)
    }

    /// key 型 PO（Http 协议，config 声明 credential_requirements）
    fn key_readiness_tool(tool_id: &str, requirements: &[CredentialRequirement]) -> Tool {
        let config = json!({
            "credential_requirements": serde_json::to_value(requirements).unwrap(),
        });
        let mut po = ToolPo::new(
            tool_id.to_string(),
            tool_id.to_string(),
            "key readiness tool".to_string(),
            ToolProtocol::Http,
            config,
            Some(json!({ "type": "object" })),
            vec!["test".to_string()],
            Some("test-user".to_string()),
        );
        po.control_mode = ControlMode::Manual;
        Tool::from_po_for_management(po)
    }

    fn tavily_default_credential() -> UserCredential {
        let mut po = UserCredentialPo::new(
            "cred-tavily-rt".to_string(),
            "org-1".to_string(),
            "test-user".to_string(),
            CredentialKind::GenericToken,
            "tavily default".to_string(),
            CredentialDetail::GenericToken {
                token: "tvly_plain".to_string(),
            },
            CredentialVisibility::Private,
            "test-user".to_string(),
        );
        po.platform = Some("tavily".to_string());
        UserCredential::from_po(po)
    }

    /// CLI 型：config.command 可寻址 → Ready
    #[tokio::test]
    async fn runtime_tool_readiness_cli_tool_with_installed_binary_is_ready() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let tool = cli_readiness_tool("rt-cli-ready", "/bin/ls", Some("install hint"));
        invalidate_readiness_cache("rt-cli-ready");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::Ready
        );
    }

    /// CLI 型：不可寻址 → NotReady{cli_not_installed}，install_hint + 工具配置双通道引导
    #[tokio::test]
    async fn runtime_tool_readiness_cli_tool_not_installed_combines_hints() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let tool = cli_readiness_tool(
            "rt-cli-missing",
            "/no/such/binary-xyz",
            Some("brew install agent-browser"),
        );
        invalidate_readiness_cache("rt-cli-missing");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "brew install agent-browser；或在工具配置中修改命令路径".to_string(),
            }
        );
    }

    /// CLI 型：存量 config 无 install_hint → hint 仅工具配置引导（零迁移兼容）
    #[tokio::test]
    async fn runtime_tool_readiness_cli_tool_without_install_hint_keeps_config_hint() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let tool = cli_readiness_tool("rt-cli-legacy", "/no/such/binary-xyz", None);
        invalidate_readiness_cache("rt-cli-legacy");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "或在工具配置中修改命令路径".to_string(),
            }
        );
    }

    /// 无 CLI 源 + 无凭据需求 → Ready（如 fs_read 等纯内置工具）
    #[tokio::test]
    async fn runtime_tool_readiness_plain_tool_without_requirements_is_ready() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let tool = Tool::from_po_for_management(test_tool_po("rt-plain", ToolProtocol::Builtin));
        invalidate_readiness_cache("rt-plain");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::Ready
        );
    }

    /// 存量 Builtin PO（config 无 command）→ 工厂默认 PO 兜底（零迁移）
    #[tokio::test]
    async fn runtime_tool_readiness_builtin_legacy_po_falls_back_to_factory_default() {
        let registry = get_registry();
        registry.register_builtin_factory(Box::new(BrowserToolFactory));
        let mut po = test_tool_po("browser", ToolProtocol::Builtin);
        po.config = json!({}); // 存量 DB 形态：sync 不刷新运维所有权字段
        let tool = Tool::from_po_for_management(po);
        invalidate_readiness_cache("browser");

        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        match runtime.tool_readiness(&test_ctx(), &tool).await {
            // 本机已安装 agent-browser → 工厂默认命令可寻址
            RuntimeReady::Ready => {}
            RuntimeReady::NotReady { reason, hint } => {
                assert_eq!(reason, "cli_not_installed");
                assert!(hint.contains("工具配置"), "hint: {}", hint);
            }
            other => panic!("unexpected readiness: {:?}", other),
        }
        registry.unregister("browser");
    }

    /// key 型（Http config 声明需求）：凭据未命中 → NotReady{api_key_missing + kind 引导}
    #[tokio::test]
    async fn runtime_tool_readiness_key_tool_missing_credential_is_not_ready() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let requirements = [credential_requirement(
            CredentialKind::GenericToken,
            None,
            Some("tavily"),
        )];
        let tool = key_readiness_tool("rt-key-miss", &requirements);
        invalidate_readiness_cache("rt-key-miss");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "绑定个人 Tavily key（设置 → 身份凭证 → 通用令牌，platform 选 tavily）"
                    .to_string(),
            }
        );
    }

    /// key 型：凭据命中（按当前查看者）→ Ready
    #[tokio::test]
    async fn runtime_tool_readiness_key_tool_with_credential_is_ready() {
        let runtime = credential_runtime(
            StubUserDal::with_default(tavily_default_credential()),
            StubLarkCredentialDal::none(),
        );
        let requirements = [credential_requirement(
            CredentialKind::GenericToken,
            None,
            Some("tavily"),
        )];
        let tool = key_readiness_tool("rt-key-hit", &requirements);
        invalidate_readiness_cache("rt-key-hit");

        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::Ready
        );
    }

    /// key 型静态声明接线：Builtin tavily_search 工厂声明 GenericToken + platform=tavily 需求
    #[tokio::test]
    async fn runtime_tool_readiness_builtin_tavily_declares_key_requirement() {
        let registry = get_registry();
        registry.register_builtin_factory(Box::new(TavilySearchToolFactory));
        let tool =
            Tool::from_po_for_management(test_tool_po("tavily_search", ToolProtocol::Builtin));
        invalidate_readiness_cache("tavily_search");

        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "绑定个人 Tavily key（设置 → 身份凭证 → 通用令牌，platform 选 tavily）"
                    .to_string(),
            }
        );
        registry.unregister("tavily_search");
    }

    /// key 型 TTL 缓存：窗口内复用上次判定（stub 变化不可见），过期后重新取数
    #[tokio::test]
    async fn runtime_tool_readiness_key_verdict_cached_until_ttl_expiry() {
        let requirements = [credential_requirement(
            CredentialKind::GenericToken,
            None,
            Some("tavily"),
        )];
        let tool = key_readiness_tool("rt-key-ttl", &requirements);
        invalidate_readiness_cache("rt-key-ttl");

        let miss = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let first = miss.tool_readiness(&test_ctx(), &tool).await;
        assert_eq!(
            first,
            RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: "绑定个人 Tavily key（设置 → 身份凭证 → 通用令牌，platform 选 tavily）"
                    .to_string(),
            }
        );

        // TTL 窗口内换命中 stub 的 runtime：缓存命中，不重新取数
        let hit = credential_runtime(
            StubUserDal::with_default(tavily_default_credential()),
            StubLarkCredentialDal::none(),
        );
        assert_eq!(hit.tool_readiness(&test_ctx(), &tool).await, first);

        // 模拟过期 → 重新判定 → Ready
        expire_readiness_cache("rt-key-ttl");
        assert_eq!(
            hit.tool_readiness(&test_ctx(), &tool).await,
            RuntimeReady::Ready
        );
    }

    /// CLI 型缓存 + 主动失效：TTL 内复用，invalidate 后按新 PO config 重判
    #[tokio::test]
    async fn runtime_tool_readiness_cli_verdict_invalidated_on_demand() {
        let runtime = credential_runtime(StubUserDal::none(), StubLarkCredentialDal::none());
        let missing = cli_readiness_tool("rt-cli-inval", "/no/such/binary-xyz", None);
        invalidate_readiness_cache("rt-cli-inval");

        let first = runtime.tool_readiness(&test_ctx(), &missing).await;
        assert_eq!(
            first,
            RuntimeReady::NotReady {
                reason: "cli_not_installed".to_string(),
                hint: "或在工具配置中修改命令路径".to_string(),
            }
        );

        // TTL 窗口内同 tool_id 换可寻址命令：缓存命中，仍旧判定
        let installed = cli_readiness_tool("rt-cli-inval", "/bin/ls", None);
        assert_eq!(runtime.tool_readiness(&test_ctx(), &installed).await, first);

        // 主动失效 → 按新 PO config 重判 → Ready
        invalidate_readiness_cache("rt-cli-inval");
        assert_eq!(
            runtime.tool_readiness(&test_ctx(), &installed).await,
            RuntimeReady::Ready
        );
    }
}
