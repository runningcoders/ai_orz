//! Runtime ToolExecution protocol routing tests.

#[cfg(test)]
mod tests {
    use crate::error::AppError;
    use crate::models::brain::Brain;
    use crate::models::memory::Memory;
    use crate::models::model_provider::ModelProvider;
    use crate::models::tool::{Tool, ToolPo};
    use crate::pkg::RequestContext;
    use crate::pkg::tool_tracing::entry::ToolCallEntry;
    use crate::service::dal::brain::BrainDal;
    use crate::service::dal::mcp_tool::McpToolDal;
    use crate::service::dal::tool::ToolDal;
    use crate::service::dao::tool::{ToolQuery, ToolSearch};
    use async_trait::async_trait;
    use common::enums::{ControlMode, ToolProtocol};
    use rig::tool::{ToolDyn, ToolError};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubBrainDal;

    #[async_trait]
    impl BrainDal for StubBrainDal {
        fn wake_brain(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _memories: Vec<Memory>,
            _tools: Vec<Tool>,
        ) -> Result<Brain, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn test_connection(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _prompt: &str,
        ) -> Result<String, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn think(
            &self,
            _ctx: RequestContext,
            _brain: &Brain,
            _prompt: &str,
        ) -> Result<String, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    struct RecordingToolDal {
        protocol: ToolProtocol,
        call_count: AtomicUsize,
    }

    impl RecordingToolDal {
        fn new(protocol: ToolProtocol) -> Self {
            Self {
                protocol,
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }

        fn tool(&self, tool_id: &str) -> Tool {
            Tool::from_po_for_management(test_tool_po(tool_id, self.protocol))
        }
    }

    #[async_trait]
    impl ToolDal for RecordingToolDal {
        async fn create_tool(&self, _ctx: RequestContext, _po: &ToolPo) -> Result<(), AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn update_tool(&self, _ctx: RequestContext, _tool: &Tool) -> Result<(), AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn delete_tool(&self, _ctx: RequestContext, _tool_id: &str) -> Result<(), AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn get_by_id(
            &self,
            _ctx: RequestContext,
            id: String,
        ) -> Result<Option<Tool>, AppError> {
            Ok(Some(self.tool(&id)))
        }

        async fn get_by_name(
            &self,
            _ctx: RequestContext,
            _name: &str,
        ) -> Result<Option<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn query(
            &self,
            _ctx: RequestContext,
            _query: ToolQuery,
        ) -> Result<Vec<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_enabled(&self, _ctx: RequestContext) -> Result<Vec<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_tools_for_agent_full(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
        ) -> Result<Vec<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn add_tool_to_agent(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _tool_id: &str,
            _created_by: Option<String>,
        ) -> Result<(), AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn remove_tool_from_agent(
            &self,
            _ctx: RequestContext,
            _agent_id: &str,
            _tool_id: &str,
        ) -> Result<(), AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn sync_builtin_tools_to_db(&self, _ctx: RequestContext) -> Result<usize, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_tool_by_id(
            &self,
            _ctx: RequestContext,
            tool_id: String,
            args: Value,
        ) -> Result<Value, ToolError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(json!({ "called_by": "tool_dal", "tool_id": tool_id, "args": args }))
        }

        async fn call_tool(
            &self,
            _ctx: RequestContext,
            _tool: &Tool,
            _args: Value,
        ) -> Result<Value, ToolError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_manual(
            &self,
            _ctx: RequestContext,
            _tool: &Tool,
            _args: Value,
        ) -> Result<(Value, ToolCallEntry), ToolError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn search(
            &self,
            _ctx: RequestContext,
            _params: ToolSearch,
        ) -> Result<Vec<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        fn wrap_for_rig(&self, _tools: &[Tool], _ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    struct RecordingMcpToolDal {
        call_count: AtomicUsize,
        error_message: Option<String>,
    }

    impl RecordingMcpToolDal {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                error_message: None,
            }
        }

        fn failing(error_message: &str) -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                error_message: Some(error_message.to_string()),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl McpToolDal for RecordingMcpToolDal {
        async fn get_by_id(
            &self,
            _ctx: RequestContext,
            _tool_id: String,
        ) -> Result<Option<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn sync_from_server(
            &self,
            _ctx: RequestContext,
            _server_id: &str,
        ) -> Result<usize, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn list_by_server(
            &self,
            _ctx: RequestContext,
            _params: common::api::ListMcpToolsByServerRequest,
        ) -> Result<common::api::PagedResult<Tool>, AppError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        async fn call_tool_by_id(
            &self,
            _ctx: RequestContext,
            tool_id: String,
            args: Value,
        ) -> Result<Value, ToolError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if let Some(error_message) = &self.error_message {
                return Err(ToolError::ToolCallError(error_message.clone().into()));
            }
            Ok(json!({ "called_by": "mcp_tool_dal", "tool_id": tool_id, "args": args }))
        }

        async fn call_manual(
            &self,
            _ctx: RequestContext,
            _tool: &Tool,
            _args: Value,
        ) -> Result<(Value, ToolCallEntry), ToolError> {
            unimplemented!("not needed by tool execution routing tests")
        }

        fn invalidate_server(&self, _server_id: &str) {
            unimplemented!("not needed by tool execution routing tests")
        }
    }

    fn test_ctx() -> RequestContext {
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        RequestContext::new_simple("test-user", pool)
    }

    fn test_tool_po(tool_id: &str, protocol: ToolProtocol) -> ToolPo {
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
        po.control_mode = ControlMode::Manual;
        po
    }

    #[tokio::test]
    async fn runtime_routes_mcp_tool_calls_to_mcp_tool_dal() {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let runtime = crate::service::domain::runtime::new_with_tool_dals(
            Arc::new(StubBrainDal),
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

        assert_eq!(result["called_by"], "mcp_tool_dal");
        assert_eq!(mcp_tool_dal.calls(), 1);
        assert_eq!(tool_dal.calls(), 0);
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
        let runtime = crate::service::domain::runtime::new_with_tool_dals(
            Arc::new(StubBrainDal),
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
        assert_eq!(mcp_tool_dal.calls(), 1);
        assert_eq!(tool_dal.calls(), 0);
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

    async fn assert_non_mcp_protocol_routes_to_generic_tool_dal(
        protocol: ToolProtocol,
        tool_id: &str,
    ) {
        let tool_dal = Arc::new(RecordingToolDal::new(protocol));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::new());
        let runtime = crate::service::domain::runtime::new_with_tool_dals(
            Arc::new(StubBrainDal),
            tool_dal.clone(),
            mcp_tool_dal.clone(),
        );

        let result = runtime
            .tool_execution()
            .call_tool_by_id(test_ctx(), tool_id.to_string(), json!({ "text": "hi" }))
            .await
            .unwrap();

        assert_eq!(result["called_by"], "tool_dal");
        assert_eq!(tool_dal.calls(), 1);
        assert_eq!(mcp_tool_dal.calls(), 0);
    }

    async fn assert_mcp_lower_error_maps_to_safe_message(lower_error: &str, expected: &str) {
        let tool_dal = Arc::new(RecordingToolDal::new(ToolProtocol::Mcp));
        let mcp_tool_dal = Arc::new(RecordingMcpToolDal::failing(lower_error));
        let runtime = crate::service::domain::runtime::new_with_tool_dals(
            Arc::new(StubBrainDal),
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
        assert_eq!(mcp_tool_dal.calls(), 1);
        assert_eq!(tool_dal.calls(), 0);
    }
}
