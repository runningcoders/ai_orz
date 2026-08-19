//! Tool DAL 单元测试
//! 测试 Tool DAL 的基础功能

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::tool::{CoreTool, Tool, ToolPo};
use crate::models::vector::MatchType;
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::{BuiltinToolFactory, get_registry};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::service::dal::tool::ToolDal;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::ModelProviderDao;
use crate::service::dao::tool;
use crate::service::dao::tool::ToolSearch;
use crate::service::dao::tool_call::ToolCallDao;
use async_trait::async_trait;
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use common::error::Result;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

// 测试用的简单工具工厂
#[derive(Clone)]
struct TestToolFactory;

impl BuiltinToolFactory for TestToolFactory {
    fn create_po(&self) -> ToolPo {
        ToolPo::new(
            "test_tool".to_string(),
            "test_tool".to_string(),
            "Test tool for unit tests".to_string(),
            ToolProtocol::Builtin,
            serde_json::Value::Null,
            None,
            vec![],
            Some("test-user".to_string()),
        )
    }
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(TestTool { po })
    }
}

// 测试用的工具
#[derive(Clone)]
struct TestTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for TestTool {
    fn po(&self) -> &ToolPo {
        &self.po
    }

    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value> {
        Ok(Value::Null)
    }
}

/// 注册测试工具工厂（每个测试开始前调用）
fn register_test_factory() {
    let registry = get_registry();
    registry.register_builtin_factory(Box::new(TestToolFactory));
}

/// 初始化测试环境
async fn init_test_env(
    pool: SqlitePool,
    register_factory: bool,
) -> (Arc<dyn ToolDal + Send + Sync>, RequestContext) {
    tool::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();

    if register_factory {
        register_test_factory();
    }

    let tool_dal = crate::service::dal::tool::new(
        tool::dao(),
        crate::service::dao::tool_call::dao(),
        tool::vector_dao(),
        crate::service::dao::model_provider::dao(),
        crate::service::dao::cortex::dao(),
        tool::stats_dao(),
    );
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (tool_dal, ctx)
}

/// 创建测试 ToolPo
fn create_test_tool_po(id: &str, name: &str, description: &str) -> ToolPo {
    ToolPo::new(
        id.to_string(),
        name.to_string(),
        description.to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        vec![],
        Some("test-user".to_string()),
    )
}

/// 测试 Tool DAL 创建和获取工具完整信息
#[sqlx::test]
async fn test_create_and_get_tool_full(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    // ========== 测试: 创建工具 ==========
    let po = create_test_tool_po("", "echo_test", "Echo test tool");

    let result = tool_dal.create_tool(ctx.clone(), &po).await;
    assert!(result.is_ok(), "create tool failed: {:?}", result);

    // ========== 测试: get_by_id 获取完整工具 ==========
    // 因为 "echo_test" 没有在 ToolRegistry 注册，所以返回 None 是正常的
    // 这正好验证了过滤逻辑
    let got = tool_dal.get_by_id(ctx.clone(), po.id.clone()).await;
    assert!(got.is_ok());
    let got = got.unwrap();
    // 未注册的内置工具无法组装，返回 None 是预期行为
    assert!(got.is_none());
}

/// 测试已存在工具的 get_by_id（对于已注册的内置工具）
#[sqlx::test]
async fn test_get_by_id_exists(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    // 创建一个测试工具
    let po = create_test_tool_po("test-builtin-id", "test-builtin", "Test builtin");
    let _ = tool_dal.create_tool(ctx.clone(), &po).await;

    // 查询完整实体 - 因为没注册，还是 None，这是预期的
    let got_full = tool_dal.get_by_id(ctx.clone(), po.id.clone()).await;
    assert!(got_full.is_ok());
    let got_full = got_full.unwrap();
    assert!(got_full.is_none());
}

/// 测试添加工具到 Agent 和列出 Agent 工具完整列表
#[sqlx::test]
async fn test_add_tool_to_agent_and_list(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, true).await;

    // 创建已注册的工具（id = test_tool）
    let test_tool = create_test_tool_po("test_tool", "test_tool", "Test tool for adding to agent");
    tool_dal.create_tool(ctx.clone(), &test_tool).await.unwrap();

    // 获取所有启用的工具（因为已经注册了 test_tool，所以至少有一个）
    let all_enabled = tool_dal.list_enabled(ctx.clone()).await.unwrap();
    assert!(!all_enabled.is_empty());

    // 创建一个虚拟 Agent
    let agent_id = Uuid::now_v7().to_string();

    // ========== 测试: 添加工具到 Agent ==========
    let result = tool_dal
        .add_tool_to_agent(
            ctx.clone(),
            &agent_id,
            "test_tool",
            Some("test-user".to_string()),
        )
        .await;
    assert!(result.is_ok(), "add tool to agent failed: {:?}", result);

    // ========== 测试: list_tools_for_agent_full (完整工具) ==========
    let list_full = tool_dal
        .list_tools_for_agent_full(ctx.clone(), &agent_id)
        .await;
    assert!(list_full.is_ok());
    let list_full = list_full.unwrap();
    // 工具已注册，所以可以正常返回
    assert_eq!(list_full.len(), 1);
    assert_eq!(list_full[0].po.id, "test_tool");
}

/// 测试 MCP Tool 可以作为标准 Tool 绑定到 Agent 并进入运行时工具列表，
/// 但默认 Manual，不会被暴露为自动调用工具。
#[sqlx::test]
async fn test_mcp_tool_bind_to_agent_visible_but_not_wrapped_for_rig(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    let mcp_tool = ToolPo::new(
        "mcp.echo-server.echo".to_string(),
        "mcp.echo-server.echo".to_string(),
        "Echo input text".to_string(),
        ToolProtocol::Mcp,
        serde_json::json!({
            "server_id": "echo-server",
            "tool_name": "echo",
            "command": "python3 /tmp/private_echo_server.py",
            "env": {"PRIVATE_VALUE": "placeholder-value"},
            "url": "https://internal.example.test/mcp"
        }),
        Some(serde_json::json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        })),
        vec!["mcp".to_string(), "echo-server".to_string()],
        Some("test-user".to_string()),
    );
    tool_dal.create_tool(ctx.clone(), &mcp_tool).await.unwrap();

    let agent_id = Uuid::now_v7().to_string();
    tool_dal
        .add_tool_to_agent(
            ctx.clone(),
            &agent_id,
            &mcp_tool.id,
            Some("test-user".to_string()),
        )
        .await
        .unwrap();

    let list_full = tool_dal
        .list_tools_for_agent_full(ctx.clone(), &agent_id)
        .await
        .unwrap();
    assert_eq!(list_full.len(), 1);
    assert_eq!(list_full[0].po.id, "mcp.echo-server.echo");
    assert_eq!(list_full[0].po.protocol, ToolProtocol::Mcp);
    assert_eq!(list_full[0].po.control_mode, ControlMode::Manual);

    // 工具列表不再注入 Prompt（通过 OpenAI tools API 协议层传递），
    // 此处仅验证工具元数据可被加载，不再断言 Prompt 内容。
    // 敏感配置（command/env/url）始终保留在 ToolPo.config 中，不暴露给模型。
}

/// 测试从 Agent 移除工具
#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, true).await;

    // 创建已注册的工具（id = test_tool）
    let test_tool = create_test_tool_po(
        "test_tool",
        "test_tool",
        "Test tool for removing from agent",
    );
    tool_dal.create_tool(ctx.clone(), &test_tool).await.unwrap();

    // 创建 Agent 并添加工具
    let agent_id = Uuid::now_v7().to_string();
    tool_dal
        .add_tool_to_agent(
            ctx.clone(),
            &agent_id,
            "test_tool",
            Some("test-user".to_string()),
        )
        .await
        .unwrap();

    // 确认添加成功
    let list_before = tool_dal
        .list_tools_for_agent_full(ctx.clone(), &agent_id)
        .await
        .unwrap();
    assert_eq!(list_before.len(), 1);

    // ========== 测试: 移除工具 ==========
    let result = tool_dal
        .remove_tool_from_agent(ctx.clone(), &agent_id, "test_tool")
        .await;
    assert!(
        result.is_ok(),
        "remove tool from agent failed: {:?}",
        result
    );

    // ========== 验证: 列表为空 ==========
    let list_after = tool_dal
        .list_tools_for_agent_full(ctx.clone(), &agent_id)
        .await
        .unwrap();
    assert!(list_after.is_empty());
}

/// 测试获取所有启用工具列表
#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, true).await;

    // 创建已注册的工具（启用，id = test_tool）
    let test_tool =
        create_test_tool_po("test_tool", "test_tool", "Test tool (enabled, registered)");

    // 创建一个未注册的禁用工具
    let mut disabled = create_test_tool_po("disabled", "disabled", "Disabled tool (disabled)");
    disabled.status = ToolStatus::Disabled;
    disabled.touch(Some("test-user".to_string()));

    tool_dal.create_tool(ctx.clone(), &test_tool).await.unwrap();
    tool_dal.create_tool(ctx.clone(), &disabled).await.unwrap();

    // 测试获取所有启用工具（只有 test_tool 是已注册且启用的）
    let tools = tool_dal.list_enabled(ctx.clone()).await;
    assert!(tools.is_ok());
    let tools = tools.unwrap();
    // 只有 test_tool 会被返回（已注册且启用）
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].po.id, "test_tool");
    // 所有返回的工具都应该是 Enabled 状态
    for tool in &tools {
        assert_eq!(tool.po.status, ToolStatus::Enabled);
    }
}

/// 测试按名称获取工具
#[sqlx::test]
async fn test_get_by_name(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    // 创建测试工具
    let po = create_test_tool_po("", "get_by_name_test", "Test get by name");
    tool_dal.create_tool(ctx.clone(), &po).await.unwrap();

    // 获取名称
    let first_name = &po.name;

    // ========== 测试: get_by_name ==========
    let got = tool_dal.get_by_name(ctx.clone(), first_name).await;
    assert!(got.is_ok());
    let got = got.unwrap();
    // 因为未注册，返回 None 是预期的
    assert!(got.is_none());

    // ========== 测试: 不存在的名称 ==========
    let got = tool_dal
        .get_by_name(ctx.clone(), "not_exists_tool_name_xxx")
        .await;
    assert!(got.is_ok());
    let got = got.unwrap();
    assert!(got.is_none());
}

/// 测试更新工具（非内置工具可以更新）
#[sqlx::test]
async fn test_update_tool(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    // 创建非内置工具
    let po = ToolPo::new(
        "".to_string(),
        "http-tool".to_string(),
        "Original description".to_string(),
        ToolProtocol::Http,
        serde_json::Value::Null,
        None,
        vec![],
        Some("test-user".to_string()),
    );

    tool_dal.create_tool(ctx.clone(), &po).await.unwrap();

    // ========== 测试: 更新工具 ==========
    let tool_dao = tool::dao();
    let mut po_to_update = tool_dao
        .get_by_id(ctx.clone(), po.id.clone())
        .await
        .unwrap()
        .unwrap();
    po_to_update.description = "Updated description".to_string();
    po_to_update.status = ToolStatus::Disabled;
    po_to_update.touch(Some("test-user".to_string()));

    let result = tool_dao.update_tool(ctx.clone(), &po_to_update).await;
    assert!(result.is_ok(), "update tool failed: {:?}", result);

    // ========== 验证: 更新生效 ==========
    let got_po = tool_dao
        .get_by_id(ctx.clone(), po.id.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got_po.description, "Updated description");
    assert_eq!(got_po.status, ToolStatus::Disabled);
}

/// 测试内置工具不能更新（保护机制）
#[sqlx::test]
async fn test_update_builtin_tool_protected(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, true).await;

    // 创建内置工具（通过 test_tool factory）
    let po = TestToolFactory {}.create_po();
    tool_dal.create_tool(ctx.clone(), &po).await.unwrap();

    // ========== 测试：直接通过 DAO 层尝试更新内置工具应该失败 ==========
    let tool_dao = tool::dao();
    let mut po_to_update = tool_dao
        .get_by_id(ctx.clone(), po.id.clone())
        .await
        .unwrap()
        .unwrap();
    po_to_update.description = "Should not work".to_string();
    po_to_update.touch(Some("test-user".to_string()));

    let result = tool_dao.update_tool(ctx.clone(), &po_to_update).await;
    assert!(result.is_err(), "update builtin tool should fail");
}

/// 测试在不存在的 ID 上调用 get_by_id 返回 None 而不是错误
#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    let not_exists_id = Uuid::now_v7().to_string();
    let result = tool_dal.get_by_id(ctx.clone(), not_exists_id).await;
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.is_none());
}

/// 测试 DAL 删除非内置工具
#[sqlx::test]
async fn test_delete_tool(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;

    let po = ToolPo::new(
        "".to_string(),
        "delete-http-tool".to_string(),
        "Tool to delete".to_string(),
        ToolProtocol::Http,
        serde_json::json!({"endpoint":"https://example.com/tool"}),
        None,
        vec![],
        Some("test-user".to_string()),
    );
    let tool_id = po.id.clone();
    tool_dal.create_tool(ctx.clone(), &po).await.unwrap();

    let result = tool_dal.delete_tool(ctx.clone(), &tool_id).await;
    assert!(result.is_ok(), "delete tool failed: {:?}", result);

    let got = tool::dao()
        .get_by_id(ctx.clone(), tool_id.clone())
        .await
        .unwrap();
    assert!(got.is_none(), "deleted tool should not exist");
}

/// 测试内置工具不能删除（保护机制）
#[sqlx::test]
async fn test_delete_builtin_tool_protected(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, true).await;

    // 创建内置工具（通过 test_tool factory）
    let po = TestToolFactory {}.create_po();
    tool_dal.create_tool(ctx.clone(), &po).await.unwrap();

    // ========== 测试：通过 DAL 层尝试删除内置工具应该失败 ==========
    let result = tool_dal.delete_tool(ctx.clone(), &po.id).await;
    assert!(result.is_err(), "delete builtin tool should fail");
}

/// 测试 sync_builtin_tools_to_db 功能
#[sqlx::test]
async fn test_sync_builtin_tools_to_db(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool.clone(), true).await;

    // ========== 测试：首次同步应该插入工具 ==========
    let inserted = tool_dal
        .sync_builtin_tools_to_db(ctx.clone())
        .await
        .unwrap();
    assert!(inserted > 0, "should insert at least one tool");

    // ========== 测试：二次同步幂等（已入库工具不重复插入） ==========
    // 注意：单元测试并行共享全局 ToolRegistry，其他测试可能在两次同步
    // 之间追加新工厂，因此不断言 inserted_again == 0；改为验证
    // 「表行数增量 == 返回的插入数」——若 sync 重复插入已有工具，
    // 行数增量将大于插入数
    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tools")
        .fetch_one(&pool)
        .await
        .unwrap();
    let inserted_again = tool_dal
        .sync_builtin_tools_to_db(ctx.clone())
        .await
        .unwrap();
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tools")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count_after - count_before,
        inserted_again as i64,
        "row growth must equal reported inserts (no duplicate tools)"
    );

    // ========== 验证：工具确实存在 ==========
    let found = tool::dao()
        .get_by_id(ctx.clone(), "test_tool".to_string())
        .await
        .unwrap();
    assert!(found.is_some(), "test_tool should exist after sync");
}

// 测试用工厂：验证 sync 刷新代码所有权字段、保留运维所有权字段
#[derive(Clone)]
struct OwnershipSyncToolFactory;

impl BuiltinToolFactory for OwnershipSyncToolFactory {
    fn create_po(&self) -> ToolPo {
        ToolPo::new(
            "ownership_sync_tool".to_string(),
            "ownership_sync_tool_v2".to_string(),
            "Fresh description from code".to_string(),
            ToolProtocol::Builtin,
            serde_json::Value::Null,
            None,
            vec!["fresh_tag".to_string()],
            Some("test-user".to_string()),
        )
    }
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(TestTool { po })
    }
}

/// 测试 sync 对存量内置工具的所有权分界刷新：
/// 代码所有权字段（name/description/tags）以代码为准刷新，
/// 运维所有权字段（config/status）保留现场设置
#[sqlx::test]
async fn test_sync_builtin_tools_refreshes_code_owned_fields(pool: SqlitePool) {
    let (tool_dal, ctx) = init_test_env(pool, false).await;
    get_registry().register_builtin_factory(Box::new(OwnershipSyncToolFactory));

    // 预插存量记录：旧的代码所有权字段 + 现场自定义的运维字段
    let mut stale = OwnershipSyncToolFactory.create_po();
    stale.name = "ownership_sync_tool_old".to_string();
    stale.description = "Old description".to_string();
    stale.tags = serde_json::to_string(&vec!["old_tag".to_string()]).unwrap();
    stale.config = serde_json::json!({"additional_allowed_paths": ["/custom/path"]});
    stale.status = common::enums::ToolStatus::Disabled;
    tool_dal.create_tool(ctx.clone(), &stale).await.unwrap();

    tool_dal
        .sync_builtin_tools_to_db(ctx.clone())
        .await
        .unwrap();

    let found = tool::dao()
        .get_by_id(ctx.clone(), "ownership_sync_tool".to_string())
        .await
        .unwrap()
        .expect("ownership_sync_tool should exist");

    // 代码所有权字段被刷新
    assert_eq!(found.name, "ownership_sync_tool_v2");
    assert_eq!(found.description, "Fresh description from code");
    assert_eq!(found.get_tags(), vec!["fresh_tag".to_string()]);

    // 运维所有权字段保留现场设置
    assert_eq!(
        found.config,
        serde_json::json!({"additional_allowed_paths": ["/custom/path"]}),
        "config should be preserved"
    );
    assert_eq!(
        found.status,
        common::enums::ToolStatus::Disabled,
        "status should be preserved"
    );
}

// ========== Mock for Search Tests（三态匹配测试） ==========
//
// MockCortexDao 的向量生成策略：
// - 包含 "nonexistent" 的文本 → 向量 [1.0, 0.0, 0.0]
// - 其他文本 → 向量 [0.0, 1.0, 1.0]
// 两向量余弦距离 = 1.0（完全正交），大于 0.8 阈值，不会同时命中。

/// Mock CortexDao（不依赖真实 LLM）
#[derive(Clone, Debug)]
struct MockCortexDao;

#[async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "Mock response".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| {
                if t.contains("nonexistent") {
                    vec![1.0, 0.0, 0.0]
                } else {
                    vec![0.0, 1.0, 1.0]
                }
            })
            .collect())
    }
}

/// Mock ModelProviderDao，返回支持 Embedding 的测试 Provider
#[derive(Clone, Debug)]
struct MockModelProviderDao;

#[async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> common::error::Result<()> {
        Ok(())
    }

    async fn find_by_id(
        &self,
        _ctx: RequestContext,
        _id: &str,
    ) -> common::error::Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        _query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> common::error::Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: vec![mock_provider()],
            total: 1,
        })
    }

    async fn find_all(&self, _ctx: RequestContext) -> common::error::Result<Vec<ModelProviderPo>> {
        Ok(vec![])
    }

    async fn update(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> common::error::Result<()> {
        Ok(())
    }

    async fn delete(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
    ) -> common::error::Result<()> {
        Ok(())
    }

    async fn get_default_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> common::error::Result<Option<ModelProviderPo>> {
        Ok(Some(mock_provider()))
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> common::error::Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo {
        id: "mock-provider".to_string(),
        name: "Mock Provider".to_string(),
        provider_type: common::enums::ProviderType::Ollama,
        model_name: "mock-embedding".to_string(),
        capability: common::enums::ModelCapability::Embedding,
        api_key: "".to_string(),
        base_url: Some("http://localhost:11434".to_string()),
        description: None,
        config: "{}".to_string(),
        status: common::enums::ModelProviderStatus::Normal,
        created_by: "system".to_string(),
        modified_by: "system".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    }
}

/// Mock ToolCallDao，让 assemble_core_tool 对任意 ToolPo 返回 TestTool
#[derive(Clone, Debug)]
struct MockToolCallDao;

#[async_trait]
impl ToolCallDao for MockToolCallDao {
    fn assemble_core_tool(
        &self,
        po: &ToolPo,
    ) -> anyhow::Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        Ok(Some(Box::new(TestTool { po: po.clone() })))
    }

    async fn execute(
        &self,
        _ctx: RequestContext,
        _tool: &Tool,
        _args: Value,
    ) -> anyhow::Result<(Value, ToolCallEntry)> {
        Ok((Value::Null, ToolCallEntry::default()))
    }
}

/// 初始化带 Mock 的测试环境（用于搜索三态匹配测试）
async fn init_test_with_mock(pool: SqlitePool) -> (Arc<dyn ToolDal>, RequestContext) {
    tool::init();

    // 创建向量元数据表（和生产环境 schema 一致）
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS vector_metadata (
            collection TEXT NOT NULL,
            source_id TEXT NOT NULL,
            content_hash TEXT,
            model TEXT,
            dimensions INTEGER,
            indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
            expire_at INTEGER,
            PRIMARY KEY (collection, source_id)
        );",
    )
    .execute(&pool)
    .await;

    // 创建 vss_tools 表（测试环境无 vss0 扩展，用普通表模拟）
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS vss_tools (
            rowid INTEGER PRIMARY KEY AUTOINCREMENT,
            embedding TEXT NOT NULL
        );",
    )
    .execute(&pool)
    .await;

    let tool_dal = crate::service::dal::tool::new(
        tool::dao(),
        Arc::new(MockToolCallDao),
        tool::vector_dao(),
        Arc::new(MockModelProviderDao),
        Arc::new(MockCortexDao),
        tool::stats_dao(),
    );
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (tool_dal, ctx)
}

/// 测试 Tool DAL search 的三态匹配（Hybrid / Vector / Keyword）
///
/// 场景设计：
/// - tool_matching：name 含 "debug"，向量 [0.0, 1.0, 1.0]
/// - tool_vector_only：name 不含 "debug"，向量 [0.0, 1.0, 1.0]
/// - 搜索关键词 "debug"：查询向量 [0.0, 1.0, 1.0]
/// - tool_matching：FTS5 命中 + 向量距离 0.0 → Hybrid
/// - tool_vector_only：FTS5 未命中 + 向量距离 0.0 → Vector
#[sqlx::test]
async fn test_search_three_state_matching(pool: SqlitePool) -> Result<()> {
    let (tool_dal, ctx) = init_test_with_mock(pool).await;

    // 1. 创建 name 含 "debug" 的工具（会同时被 FTS5 和向量命中 → Hybrid）
    let po_matching = create_test_tool_po("", "debug-helper", "Helps with debugging");
    tool_dal.create_tool(ctx.clone(), &po_matching).await?;

    // 2. 创建 name 不含 "debug" 的工具（只被向量命中 → Vector）
    let po_vector_only = create_test_tool_po("", "python-utility", "A python utility tool");
    tool_dal.create_tool(ctx.clone(), &po_vector_only).await?;

    // 3. 搜索 "debug"（trigram 需要 3+ 字符，"debug" 是 5 字符，OK）
    let page = tool_dal
        .search(
            ctx.clone(),
            ToolSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    let results = page.items;

    // 应返回 2 条结果（Hybrid + Vector）
    assert_eq!(results.len(), 2, "应返回 Hybrid + Vector 共 2 条结果");

    // 第一条应是 Hybrid（优先级最高）
    assert_eq!(results[0].po.id, po_matching.id);
    assert_eq!(
        results[0].search_match.as_ref().unwrap().match_type,
        MatchType::Hybrid,
        "tool_matching 应是 Hybrid 匹配"
    );
    assert!(
        results[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(results[0].search_match.as_ref().unwrap().fts_rank.is_some());

    // 第二条应是 Vector（仅向量命中）
    assert_eq!(results[1].po.id, po_vector_only.id);
    assert_eq!(
        results[1].search_match.as_ref().unwrap().match_type,
        MatchType::Vector,
        "tool_vector_only 应是 Vector 匹配"
    );
    assert!(
        results[1]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_some()
    );
    assert!(results[1].search_match.as_ref().unwrap().fts_rank.is_none());

    Ok(())
}

/// 测试 Tool DAL search 的 Keyword-only 匹配
///
/// 当工具内容含 "nonexistent" 时，向量 [1.0, 0.0, 0.0]
/// 搜索 "nonexistent" 时查询向量也是 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// 向量距离 = 0.0 < 0.8 → 向量命中
///
/// 为了测试纯 Keyword 匹配，需要向量不命中的场景：
/// - 工具含 "nonexistent" → 向量 [1.0, 0.0, 0.0]
/// - 搜索 "nonexistent-debug" → 不含 "nonexistent"... 等等，含
///
/// 换个策略：搜索不含 "nonexistent" 的关键词，但工具含 "nonexistent"
/// - 工具 name = "nonexistent-debug-tool" → 向量 [1.0, 0.0, 0.0]（含 "nonexistent"）
/// - 搜索 "debug" → 查询向量 [0.0, 1.0, 1.0]（不含 "nonexistent"）
/// - 向量距离 = 1.0 > 0.8 → 向量不命中
/// - FTS5 命中 "debug" → Keyword-only
#[sqlx::test]
async fn test_search_keyword_only_match(pool: SqlitePool) -> Result<()> {
    let (tool_dal, ctx) = init_test_with_mock(pool).await;

    // 创建 name 含 "nonexistent" 和 "debug" 的工具
    let po = create_test_tool_po(
        "",
        "nonexistent-debug-tool",
        "A tool for nonexistent debugging",
    );
    tool_dal.create_tool(ctx.clone(), &po).await?;

    // 搜索 "debug"（查询向量 [0.0, 1.0, 1.0]，工具向量 [1.0, 0.0, 0.0]，距离 1.0 > 0.8）
    let page = tool_dal
        .search(
            ctx.clone(),
            ToolSearch {
                keyword: Some("debug".to_string()),
                ..Default::default()
            },
        )
        .await?;
    let results = page.items;

    // 应返回 1 条结果（Keyword-only）
    assert_eq!(results.len(), 1, "应返回 1 条 Keyword 匹配结果");

    assert_eq!(results[0].po.id, po.id);
    assert_eq!(
        results[0].search_match.as_ref().unwrap().match_type,
        MatchType::Keyword,
        "应是 Keyword 匹配（向量距离 > 阈值）"
    );
    assert!(results[0].search_match.as_ref().unwrap().fts_rank.is_some());
    assert!(
        results[0]
            .search_match
            .as_ref()
            .unwrap()
            .vector_distance
            .is_none()
    );

    Ok(())
}
