//! Runtime ToolExecution protocol routing tests.

#[cfg(test)]
mod tests {
        use common::error::Error;
    use common::models::{AgentStats, ModelCallStats, StatsFetchOptions, ToolStats};
    use crate::models::agent::{Agent, AgentPo};
    use crate::models::brain::Brain;
    use crate::models::memory::Memory;
    use crate::models::model_provider::ModelProvider;
    use crate::models::tool::{Tool, ToolCallTraceRef, ToolPo};
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
    use common::enums::{AgentStatus, ControlMode, ToolProtocol, ToolStatus};
    use rig::tool::{ToolDyn, ToolError};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use common::error::Result;

    struct StubBrainDal;

    #[async_trait]
    impl BrainDal for StubBrainDal {
        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &AgentPo,
            _memories: Vec<Memory>,
            _tools: Vec<Tool>,
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
            _prompt: &str,
        ) -> Result<String> {
            unimplemented!("not needed by tool execution routing tests")
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
            Self { agent: Some(test_agent_with_installed_tags("test-agent", vec![])) }
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

        async fn query(&self, _ctx: RequestContext, _query: AgentQuery) -> Result<common::api::PagedResult<Agent>> {
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

        async fn search(&self, _ctx: RequestContext, _search: AgentSearch) -> Result<Vec<Agent>> {
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

        async fn get_by_id(
            &self,
            _ctx: RequestContext,
            id: String,
        ) -> Result<Option<Tool>> {
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

        async fn get_by_name(
            &self,
            _ctx: RequestContext,
            _name: &str,
        ) -> Result<Option<Tool>> {
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

        async fn call_tool_by_id(
            &self,
            _ctx: RequestContext,
            tool_id: String,
            args: Value,
            ) -> Result<(Value, ToolCallEntry)> {
            self.call_by_id_count.fetch_add(1, Ordering::SeqCst);
            let entry = ToolCallEntry {
                tool_id: tool_id.clone(),
                call_id: "test-call-id".to_string(),
                ..Default::default()
            };
            Ok((
                json!({ "called_by": "tool_dal", "tool_id": tool_id, "args": args }),
                entry,
            ))
            }

        async fn call_tool(
            &self,
            _ctx: RequestContext,
            tool: &Tool,
            args: Value,
            ) -> Result<(Value, ToolCallEntry)> {
            self.call_tool_count.fetch_add(1, Ordering::SeqCst);
            let entry = ToolCallEntry {
                tool_id: tool.po.id.clone(),
                call_id: "test-call-id".to_string(),
                ..Default::default()
            };
            Ok((
                json!({ "called_by": "tool_dal", "tool_id": tool.po.id, "args": args }),
                entry,
            ))
            }

        async fn call_manual(
            &self,
            _ctx: RequestContext,
            _tool: &Tool,
            _args: Value,
            ) -> Result<(Value, ToolCallEntry)> {
            unimplemented!("not needed by tool execution routing tests")
            }

        async fn search(
            &self,
            _ctx: RequestContext,
            _params: ToolSearch,
            ) -> Result<Vec<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
            }

        fn wrap_for_rig(&self, _tools: &[Tool], _ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_stats(&self, _ctx: RequestContext, _tool_id: &str, _options: StatsFetchOptions) -> Result<ToolStats> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn rebuild_vectors(&self, _ctx: RequestContext) -> Result<()> {
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
        async fn get_by_id(
            &self,
            _ctx: RequestContext,
            _tool_id: String,
        ) -> Result<Option<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn sync_from_server(
            &self,
            _ctx: RequestContext,
            _server_id: &str,
        ) -> Result<usize> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_by_server(
            &self,
            _ctx: RequestContext,
            _params: common::api::ListMcpToolsByServerRequest,
        ) -> Result<common::api::PagedResult<Tool>> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_tool_by_id(
            &self,
            _ctx: RequestContext,
            tool_id: String,
            args: Value,
        ) -> Result<(Value, ToolCallEntry)> {
            self.call_by_id_count.fetch_add(1, Ordering::SeqCst);
            if let Some(error_message) = &self.error_message {
                let error = ToolError::ToolCallError(error_message.clone().into());
                return Err(match &self.error_trace_ref {
                    Some(trace_ref) => {
                        use common::error::{ErrorCode, ErrorType};
                        let mut err = common::error::Error::typed(
                            ErrorCode::ToolExecutionFailed,
                            ErrorType::Tool,
                            error.to_string(),
                        ).with_source(error);
                        let mut field = common::error::ErrorField::new();
                        field.insert("trace_ref".to_string(), serde_json::to_value(trace_ref.clone()).unwrap_or_default());
                        err = err.with_field(field);
                        err
                    },
                        None => {
                        common::error::Error::tool_call_failed(error.to_string()).with_source(error)
                    },
                });
                        }
            Ok((
                json!({ "called_by": "mcp_tool_dal", "tool_id": tool_id, "args": args }),
                ToolCallEntry {
                    tool_id: tool_id.clone(),
                    call_id: "test-call-id".to_string(),
                    ..Default::default()
                },
            ))
                    }

        async fn call_tool(
            &self,
            _ctx: RequestContext,
            tool: &Tool,
            args: Value,
                    ) -> Result<(Value, ToolCallEntry)> {
            self.call_tool_count.fetch_add(1, Ordering::SeqCst);
                        if let Some(error_message) = &self.error_message {
                let error = ToolError::ToolCallError(error_message.clone().into());
                            return Err(match &self.error_trace_ref {
                                Some(trace_ref) => {
                        use common::error::{ErrorCode, ErrorType};
                        let mut err = common::error::Error::typed(
                            ErrorCode::ToolExecutionFailed,
                            ErrorType::Tool,
                            error.to_string(),
                        ).with_source(error);
                        let mut field = common::error::ErrorField::new();
                        field.insert("trace_ref".to_string(), serde_json::to_value(trace_ref.clone()).unwrap_or_default());
                        err = err.with_field(field);
                        err
                    },
                                    None => {
                        common::error::Error::tool_call_failed(error.to_string()).with_source(error)
                    },
                });
                                    }
            Ok((
                json!({ "called_by": "mcp_tool_dal", "tool_id": tool.po.id, "args": args }),
                ToolCallEntry {
                    tool_id: tool.po.id.clone(),
                    call_id: "test-call-id".to_string(),
                    ..Default::default()
                },
            ))
                                }

        async fn call_manual(
            &self,
            _ctx: RequestContext,
            tool: &Tool,
            args: Value,
                                ) -> Result<(Value, ToolCallEntry)> {
            Ok((
                json!({ "called_by": "mcp_tool_dal", "tool_id": tool.po.id, "args": args }),
                ToolCallEntry {
                    tool_id: tool.po.id.clone(),
                    call_id: "test-call-id".to_string(),
                    ..Default::default()
                },
            ))
                                }

        fn invalidate_server(&self, _server_id: &str) {
            unimplemented!("not needed by tool execution routing tests")
                            }
                        }

        fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
                    }

        fn test_runtime_with_tool_dals(
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    ) -> (tempfile::TempDir, Arc<dyn crate::service::domain::runtime::RuntimeDomain>) {
        test_runtime_with_all(tool_dal, mcp_tool_dal, Arc::new(StubAgentDal::new()))
    }

    fn test_runtime_with_all(
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        agent_dal: Arc<dyn AgentDal>,
    ) -> (tempfile::TempDir, Arc<dyn crate::service::domain::runtime::RuntimeDomain>) {
        let temp_dir = tempdir().expect("tempdir should be created");
        let logger = Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf()));
        let runtime = crate::service::domain::runtime::new_with_all(
            Arc::new(StubBrainDal),
            tool_dal,
            mcp_tool_dal,
            agent_dal,
            logger,
        );
        (temp_dir, runtime)
    }



        fn scoped_test_ctx(agent_id: &str, project_id: &str, task_id: &str) -> RequestContext {
            test_ctx()
                .to_builder()
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
        );

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(test_ctx(), ToolCallQuery::default())
            .await
            .expect_err("unscoped tool call query must fail closed");

        assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
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
        );

        let error = runtime
            .tool_execution()
            .query_tool_call_entries(
                test_ctx(),
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
        );
        let mut ctx = test_ctx();
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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );
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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let (_temp_dir, runtime) = test_runtime_with_tool_dals(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

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
        let tool_dal = Arc::new(
            RecordingToolDal::with_bound_tools(vec![])
                .with_all_tools(vec![tool_po]),
        );
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let agent_dal = Arc::new(StubAgentDal::with_agent(test_agent_with_installed_tags(
            "agent-1",
            vec!["project_management"],
        )));
        let (_temp_dir, runtime) = test_runtime_with_all(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
            agent_dal,
        );

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
        let tool_dal = Arc::new(
            RecordingToolDal::with_bound_tools(vec![])
                .with_all_tools(vec![tool_po]),
        );
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let agent_dal = Arc::new(StubAgentDal::with_agent(test_agent_with_installed_tags(
            "agent-1",
            vec!["project_management"],
        )));
        let (_temp_dir, runtime) = test_runtime_with_all(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
            agent_dal,
        );

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
        let tool_dal = Arc::new(
            RecordingToolDal::with_bound_tools(vec![])
                .with_all_tools(vec![tool_po]),
        );
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        // StubAgentDal::new() 返回带空 installed_tags 的 Agent
        let agent_dal = Arc::new(StubAgentDal::new());
        let (_temp_dir, runtime) = test_runtime_with_all(
            tool_dal.clone(),
            mcp_tool_dal.clone(),
            agent_dal,
        );

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

                                        }
