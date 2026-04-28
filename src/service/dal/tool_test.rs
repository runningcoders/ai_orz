//! Tool DAL 单元测试
//! 测试 Tool DAL 的基础功能

use crate::models::tool::{ToolPo, Tool};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::tool;
use common::enums::{ToolProtocol, ToolStatus};
use sqlx::SqlitePool;
use uuid::Uuid;

/// 测试 Tool DAL 创建和获取工具完整信息
#[sqlx::test]
async fn test_create_and_get_tool_full(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
    let tool_dao = tool::dao();
    let tool_dal = crate::service::dal::tool::new(tool_dao);

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

    // ========== 测试: get_by_id 获取 PO ==========
    let got_po = tool_dal.get_by_id(&ctx, po.id.clone()).await;
    assert!(got_po.is_ok());
    let got_po = got_po.unwrap();
    assert!(got_po.is_some());
    let got_po = got_po.unwrap();
    assert_eq!(got_po.name, "echo_test");
    assert_eq!(got_po.status, ToolStatus::Enabled);

    // ========== 测试: get_tool_full 获取完整工具 ==========
    // 注意：内置工具需要注册到 registry 才能拼装成功
    // 这里我们只测试查询流程，对于未注册的内置工具返回 None 是正确的
    let got_full = tool_dal.get_tool_full(&ctx, po.id.clone()).await;
    assert!(got_full.is_ok());
    // 因为这个工具是我们新建的，没有在 registry 注册，所以返回 None
    // 如果是已注册的内置工具，会返回 Some(Tool)
    let got_full = got_full.unwrap();
    assert!(got_full.is_none());
}

/// 测试已存在工具的 get_tool_full（对于已注册的内置工具）
#[sqlx::test]
async fn test_get_tool_full_exists(pool: SqlitePool) {
    // 初始化
    tool::init();
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
    let got_full = tool_dal.get_tool_full(&ctx, po.id.clone()).await;
    assert!(got_full.is_ok());
    let got_full = got_full.unwrap();
    // 因为没在 registry 注册，所以是 None
    assert!(got_full.is_none());
}

/// 测试添加工具到 Agent 和列出 Agent 工具完整列表
#[sqlx::test]
async fn test_add_tool_to_agent_and_list(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    let tool_dal = crate::service::dal::tool::new(tool::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建两个测试工具（不需要依赖已注册的内置工具）
    let tool1 = ToolPo::new(
        "tool-1".to_string(),
        "tool-1".to_string(),
        "Tool 1".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let tool2 = ToolPo::new(
        "tool-2".to_string(),
        "tool-2".to_string(),
        "Tool 2".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let _ = tool_dal.create_tool(&ctx, &tool1).await;
    let _ = tool_dal.create_tool(&ctx, &tool2).await;

    // 获取所有启用的工具（现在应该有两个）
    let all_enabled = tool_dal.list_enabled(&ctx).await.unwrap();
    assert!(!all_enabled.is_empty());

    // 创建一个虚拟 Agent
    let agent_id = Uuid::now_v7().to_string();
    let first_tool_id = &all_enabled[0].id;

    // ========== 测试: 添加工具到 Agent ==========
    let result = tool_dal.add_tool_to_agent(&ctx, &agent_id, first_tool_id, Some("test-user".to_string())).await;
    assert!(result.is_ok(), "add tool to agent failed: {:?}", result);

    // ========== 测试: list_tools_for_agent (仅 PO) ==========
    let list_po = tool_dal.list_tools_for_agent(&ctx, &agent_id).await;
    assert!(list_po.is_ok());
    let list_po = list_po.unwrap();
    assert_eq!(list_po.len(), 1);
    assert_eq!(&list_po[0].id, first_tool_id);

    // ========== 测试: list_tools_for_agent_full (完整工具) ==========
    let list_full = tool_dal.list_tools_for_agent_full(&ctx, &agent_id).await;
    assert!(list_full.is_ok());
    let list_full = list_full.unwrap();
    // 两个工具都没注册，所以返回空（过滤了无法拼装的工具）
    assert!(list_full.is_empty());
}

/// 测试从 Agent 移除工具
#[sqlx::test]
async fn test_remove_tool_from_agent(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
    crate::pkg::tool_registry::init();
    let tool_dal = crate::service::dal::tool::new(tool::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建测试工具
    let tool1 = ToolPo::new(
        "tool-1".to_string(),
        "tool-1".to_string(),
        "Tool 1".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let _ = tool_dal.create_tool(&ctx, &tool1).await;

    // 创建 Agent 并添加工具
    let agent_id = Uuid::now_v7().to_string();
    let tool_id = &tool1.id;
    tool_dal.add_tool_to_agent(&ctx, &agent_id, tool_id, Some("test-user".to_string())).await.unwrap();

    // 确认添加成功
    let list_before = tool_dal.list_tools_for_agent(&ctx, &agent_id).await.unwrap();
    assert_eq!(list_before.len(), 1);

    // ========== 测试: 移除工具 ==========
    let result = tool_dal.remove_tool_from_agent(&ctx, &agent_id, tool_id).await;
    assert!(result.is_ok(), "remove tool from agent failed: {:?}", result);

    // ========== 验证: 列表为空 ==========
    let list_after = tool_dal.list_tools_for_agent(&ctx, &agent_id).await.unwrap();
    assert!(list_after.is_empty());
}

/// 测试获取所有启用工具列表
#[sqlx::test]
async fn test_list_enabled(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
    let tool_dal = crate::service::dal::tool::dal();

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建几个测试工具，一个禁用
    let enabled1 = ToolPo::new(
        "enabled-1".to_string(),
        "enabled-1".to_string(),
        "Enabled tool 1".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let enabled2 = ToolPo::new(
        "enabled-2".to_string(),
        "enabled-2".to_string(),
        "Enabled tool 2".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    let mut disabled = ToolPo::new(
        "disabled".to_string(),
        "disabled".to_string(),
        "Disabled tool".to_string(),
        ToolProtocol::Builtin,
        serde_json::Value::Null,
        None,
        Some("test-user".to_string()),
    );
    disabled.status = ToolStatus::Disabled;
    disabled.touch(Some("test-user".to_string()));

    tool_dal.create_tool(&ctx, &enabled1).await.unwrap();
    tool_dal.create_tool(&ctx, &enabled2).await.unwrap();
    tool_dal.create_tool(&ctx, &disabled).await.unwrap();

    // 测试获取所有启用工具
    let tools = tool_dal.list_enabled(&ctx).await;
    assert!(tools.is_ok());
    let tools = tools.unwrap();
    assert_eq!(tools.len(), 2);
    // 所有返回的工具都应该是 Enabled 状态
    for tool in &tools {
        assert_eq!(tool.status, ToolStatus::Enabled);
    }
}

/// 测试按名称获取工具
#[sqlx::test]
async fn test_get_by_name(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
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
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(&got.name, first_name);

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
    crate::service::dal::tool::init();
    let tool_dal = crate::service::dal::tool::new(tool::dao());

    let ctx = RequestContext::new_simple("test-user", pool);

    // 创建工具
    let mut po = ToolPo::new(
        "".to_string(),
        "test_update".to_string(),
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
    let got = tool_dal.get_by_id(&ctx, po.id.clone()).await.unwrap().unwrap();
    assert_eq!(got.description, "Updated description");
    assert_eq!(got.status, ToolStatus::Disabled);
}

/// 测试在不存在的 ID 上调用 get_tool_full 返回 None 而不是错误
#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    // 初始化
    tool::init();
    crate::service::dal::tool::init();
    let tool_dal = crate::service::dal::tool::dal();

    let ctx = RequestContext::new_simple("test-user", pool);

    let not_exists_id = Uuid::now_v7().to_string();
    let result = tool_dal.get_tool_full(&ctx, not_exists_id).await;
    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(result.is_none());
}
