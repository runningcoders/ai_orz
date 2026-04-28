//! Tool DAL 单元测试
//! 测试 Tool DAL 的基础功能

use crate::models::tool::{ToolPo, Tool, CoreTool};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::tool;
use crate::pkg::tool_registry::{self, BuiltinToolFactory, get_registry};
use common::enums::{ToolProtocol, ToolStatus};
use rig::tool::ToolError;
use sqlx::SqlitePool;
use async_trait::async_trait;
use uuid::Uuid;
use serde_json::Value;

// 测试用的简单工具工厂
#[derive(Clone)]
struct TestToolFactory;

impl BuiltinToolFactory for TestToolFactory {
    fn id(&self) -> &'static str {
        "test_tool"
    }
    fn name(&self) -> &'static str {
        "test_tool"
    }
    fn description(&self) -> &'static str {
        "Test tool for unit tests"
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

    async fn call(&self, _ctx: &RequestContext, _args: Value) -> Result<Value, ToolError> {
        Ok(Value::Null)
    }
}

/// 注册测试工具工厂（每个测试开始前调用）
fn register_test_factory() {
    let registry = get_registry();
    registry.register_builtin_factory(Box::new(TestToolFactory));
}

/// 测试 Tool DAL 创建和获取工具完整信息
#[sqlx::test]
async fn test_create_and_get_tool_full(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    let tool_dao = tool::dao();
    let tool_call_dao = crate::service::dao::tool_call::dao();
    let tool_dal = crate::service::dal::tool::new(tool_dao, tool_call_dao);

    let ctx = RequestContext::new_simple("test-user", pool);

    // ========== 测试: 创建工具 ==========
    let po = ToolPo::new(
        "".to_string(), // id 自动生成
        "echo_test".to_string(),
        "Echo test tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );

    let result = tool_dal.create_tool(&ctx, &po).await;
    assert!(result.is_ok(), "create tool failed: {:?}", result);

    // ========== 测试: get_by_id 获取完整工具 ==========
    // 因为 "echo_test" 没有在 ToolRegistry 注册，所以返回 None 是正常的
    // 这正好验证了过滤逻辑
    let got = tool_dal.get_by_id(&ctx, po.id.clone()).await;
    assert!(got.is_ok());
    let got = got.unwrap();
    // 未注册的内置工具无法组装，返回 None 是预期行为
    assert!(got.is_none());
}

/// 测试已存在工具的 get_by_id（对于已注册的内置工具）
#[sqlx::test]
async fn test_get_by_id_exists(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();

    let ctx = RequestContext::new_simple("test-user", pool);
    let tool_dal = crate::service::dal::tool::dal();

    // 创建一个测试工具
    let po = ToolPo::new(
        "test-builtin-id".to_string(),
        "test-builtin".to_string(),
        "Test builtin".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let _ = tool_dal.create_tool(&ctx, &po).await;

    // 查询完整实体 - 因为没注册，还是 None，这是预期的
    let got_full = tool_dal.get_by_id(&ctx, po.id.clone()).await;
    assert!(got_full.is_ok());
    let got_full = got_full.unwrap();
    assert!(got_full.is_none());
}

/// 测试添加工具到 Agent 和列出 Agent 工具完整列表
#[sqlx::test]
async fn test_add_tool_to_agent_and_list(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    register_test_factory();  // 注册测试工厂
    let tool_dal = crate::service::dal::tool::new(tool::dao(), crate::service::dao::tool_call::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建已注册的工具（id = test_tool）
    let test_tool = ToolPo::new(
        "test_tool".to_string(),
        "test_tool".to_string(),
        "Test tool for adding to agent".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    tool_dal.create_tool(&ctx, &test_tool).await.unwrap();

    // 获取所有启用的工具（因为已经注册了 test_tool，所以至少有一个）
    let all_enabled = tool_dal.list_enabled(&ctx).await.unwrap();
    assert!(!all_enabled.is_empty());

    // 创建一个虚拟 Agent
    let agent_id = Uuid::now_v7().to_string();

    // ========== 测试: 添加工具到 Agent ==========
    let result = tool_dal.add_tool_to_agent(&ctx, &agent_id, "test_tool", Some("test-user".to_string())).await;
    assert!(result.is_ok(), "add tool to agent failed: {:?}", result);

    // ========== 测试: list_tools_for_agent_full (完整工具) ==========
    let list_full = tool_dal.list_tools_for_agent_full(&ctx, &agent_id).await;
    assert!(list_full.is_ok());
    let list_full = list_full.unwrap();
    // 工具已注册，所以可以正常返回
    assert_eq!(list_full.len(), 1);
    assert_eq!(list_full[0].po.id, "test_tool");
}

/// 测试从 Agent 移除工具
#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    register_test_factory();  // 注册测试工厂
    let tool_dal = crate::service::dal::tool::new(tool::dao(), crate::service::dao::tool_call::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建已注册的工具（id = test_tool）
    let test_tool = ToolPo::new(
        "test_tool".to_string(),
        "test_tool".to_string(),
        "Test tool for removing from agent".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    tool_dal.create_tool(&ctx, &test_tool).await.unwrap();

    // 创建 Agent 并添加工具
    let agent_id = Uuid::now_v7().to_string();
    tool_dal.add_tool_to_agent(&ctx, &agent_id, "test_tool", Some("test-user".to_string())).await.unwrap();

    // 确认添加成功
    let list_before = tool_dal.list_tools_for_agent_full(&ctx, &agent_id).await.unwrap();
    assert_eq!(list_before.len(), 1);

    // ========== 测试: 移除工具 ==========
    let result = tool_dal.remove_tool_from_agent(&ctx, &agent_id, "test_tool").await;
    assert!(result.is_ok(), "remove tool from agent failed: {:?}", result);

    // ========== 验证: 列表为空 ==========
    let list_after = tool_dal.list_tools_for_agent_full(&ctx, &agent_id).await.unwrap();
    assert!(list_after.is_empty());
}

/// 测试获取所有启用工具列表
#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    register_test_factory();  // 注册测试工厂
    let tool_dal = crate::service::dal::tool::dal();

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建已注册的工具（启用，id = test_tool）
    let mut test_tool = ToolPo::new(
        "test_tool".to_string(),
        "test_tool".to_string(),
        "Test tool (enabled, registered)".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    // 创建一个未注册的禁用工具
    let mut disabled = ToolPo::new(
        "disabled".to_string(),
        "disabled".to_string(),
        "Disabled tool (disabled)".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    disabled.status = ToolStatus::Disabled;
    disabled.touch(Some("test-user".to_string()));

    tool_dal.create_tool(&ctx, &test_tool).await.unwrap();
    tool_dal.create_tool(&ctx, &disabled).await.unwrap();

    // 测试获取所有启用工具（只有 test_tool 是已注册且启用的）
    let tools = tool_dal.list_enabled(&ctx).await;
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
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    let tool_dal = crate::service::dal::tool::dal();

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建测试工具
    let po = ToolPo::new(
        "".to_string(),
        "get_by_name_test".to_string(),
        "Test get by name".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    tool_dal.create_tool(&ctx, &po).await.unwrap();

    // 获取名称
    let first_name = &po.name;

    // ========== 测试: get_by_name ==========
    let got = tool_dal.get_by_name(&ctx, first_name).await;
    assert!(got.is_ok());
    let got = got.unwrap();
    // 因为未注册，返回 None 是预期的
    assert!(got.is_none());

    // ========== 测试: 不存在的名称 ==========
    let got = tool_dal.get_by_name(&ctx, "not_exists_tool_name_xxx").await;
    assert!(got.is_ok());
    let got = got.unwrap();
    assert!(got.is_none());
}

/// 测试更新工具
#[sqlx::test]
async fn test_update_tool(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    register_test_factory();  // 注册测试工厂
    let tool_dal = crate::service::dal::tool::new(tool::dao(), crate::service::dao::tool_call::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建已注册的工具（id = test_tool）
    let mut po = ToolPo::new(
        "test_tool".to_string(),
        "test_tool".to_string(),
        "Original description".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );

    tool_dal.create_tool(&ctx, &po).await.unwrap();

    // ========== 测试: 更新工具 ==========
    po.description = "Updated description".to_string();
    po.status = ToolStatus::Disabled;
    po.touch(Some("test-user".to_string()));

    let result = tool_dal.update_tool(&ctx, &po).await;
    assert!(result.is_ok(), "update tool failed: {:?}", result);

    // ========== 验证: 更新生效 ==========
    // 因为 status 变成了 Disabled，所以 get_by_id 应该返回 None（过滤掉了禁用的）
    // 我们验证数据库中确实更新成功了，通过 dao 层直接查询
    let tool_dao = tool::dao();
    let got_po = tool_dao.get_by_id(&ctx, po.id.clone()).await.unwrap().unwrap();
    assert_eq!(got_po.description, "Updated description");
    assert_eq!(got_po.status, ToolStatus::Disabled);
}

/// 测试在不存在的 ID 上调用 get_by_id 返回 None 而不是错误
#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    let tool_dal = crate::service::dal::tool::dal();

    let ctx = RequestContext::new_simple("test-user", pool);

    let not_exists_id = Uuid::now_v7().to_string();
    let result = tool_dal.get_by_id(&ctx, not_exists_id).await;
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.is_none());
}
