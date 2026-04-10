//! Agent DAO SQLite 单元测试

use crate::models::agent::AgentPo;
use crate::pkg::storage;
use crate::pkg::storage::sql;
use common::constants::RequestContext;
use crate::service::dao::agent::{AgentDaoTrait, sqlite::AgentDaoImpl};
use uuid::Uuid;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

/// 测试所有 Agent DAO 功能
///
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[test]
fn test_all_agent_dao_functions() {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_agent_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表和索引
    let _ = storage::get().conn().execute(sql::SQLITE_CREATE_TABLE_AGENTS, ());
    // 主键自动创建索引，不需要额外创建

    let ctx = RequestContext::new(Some("admin".to_string()), None);
    let agent_dao = AgentDaoImpl::new();

    // ========== 测试 1: 插入并查询
    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        Some("worker".to_string()),
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let result = agent_dao.insert(ctx.clone(), &agent_po);
    assert!(result.is_ok());

    // 验证插入成功（使用 DAO 接口查询，不是直接 SQL）
    let found = agent_dao.find_by_id(ctx.clone(), &agent_po.id);
    assert!(found.is_ok());
    let found_agent = found.unwrap();
    assert!(found_agent.is_some());
    let found_agent = found_agent.unwrap();
    assert_eq!(found_agent.name, "TestAgent");
    assert_eq!(found_agent.created_by, "admin");

    // ========== 测试 2: 查询全部
    // 插入另外两个 Agent（全部通过 DAO 接口插入）
    for i in 0..2 {
        let agent_po2 = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        let _ = agent_dao.insert(ctx.clone(), &agent_po2);
    }

    let all = agent_dao.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 3);

    // ========== 测试 3: 更新
    let mut updated = found_agent;
    updated.name = "UpdatedAgent".to_string();
    updated.modified_by = "editor".to_string();
    let result = agent_dao.update(new_ctx("editor"), &updated);
    assert!(result.is_ok());

    let found_after_update = agent_dao.find_by_id(ctx.clone(), &updated.id).unwrap().unwrap();
    assert_eq!(found_after_update.name, "UpdatedAgent");
    assert_eq!(found_after_update.modified_by, "editor");

    // ========== 测试 4: 软删除
    assert!(agent_dao.delete(new_ctx("admin"), &updated).is_ok());
    let found_after_delete = agent_dao.find_by_id(ctx.clone(), &updated.id).unwrap();
    assert!(found_after_delete.is_none());

    // ========== 测试 5: find_all 排除已删除
    // 现有两个未删除，一个已删除
    let result = agent_dao.find_all(ctx.clone()).unwrap();
    assert_eq!(result.len(), 2);
    let names: Vec<String> = result.iter().map(|a| a.name.clone()).collect();
    assert!(names.contains(&"Agent0".to_string()));
    assert!(names.contains(&"Agent1".to_string()));

    // ========== 测试 6: 查询不存在
    let found_none = agent_dao.find_by_id(ctx.clone(), "not-exist-id").unwrap();
    assert!(found_none.is_none());
}
