//! Agent DAL 单元测试

use crate::service::dal::agent::{AgentDal, AgentDalTrait};
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::storage;
use common::constants::RequestContext;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

pub fn setup_test_dal() -> Arc<dyn AgentDalTrait> {
    // 使用随机文件名，避免冲突
    let random_name = format!("/tmp/ai_orz_test_agent_dal_{}.db", Uuid::now_v7());
    let _ = std::fs::remove_file(&random_name);
    let _ = storage::init(&random_name);

    // 创建表
    let _ = storage::get().conn().execute(crate::pkg::storage::sql::SQLITE_CREATE_TABLE_AGENTS, ());

    // 使用 DAO 的初始化方法
    crate::service::dao::agent::init();

    Arc::new(AgentDal::new(crate::service::dao::agent::dao()))
}

/// 测试所有 Agent DAL 功能
/// 
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[test]
fn test_all_agent_dal_functions() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

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
    let agent = Agent::from_po(agent_po);

    dal.create(ctx.clone(), &agent).unwrap();
    let found = dal.find_by_id(ctx.clone(), &agent.id()).unwrap().unwrap();

    assert_eq!(found.name(), "TestAgent");
    assert_eq!(found.po.created_by, "admin");

    // ========== 测试 2: 查询全部
    // 插入另外两个 Agent（全部通过 DAL 接口插入）
    for i in 0..2 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        dal.create(ctx.clone(), &agent).unwrap();
    }

    let all = dal.find_all(ctx.clone()).unwrap();
    assert_eq!(all.len(), 3);

    // ========== 测试 3: 更新
    let mut updated = found.clone();
    updated.po.name = "UpdatedAgent".to_string();
    dal.update(new_ctx("editor"), &updated).unwrap();

    let found_after_update = dal.find_by_id(ctx.clone(), &updated.id()).unwrap().unwrap();
    assert_eq!(found_after_update.name(), "UpdatedAgent");
    assert_eq!(found_after_update.po.modified_by, "editor");

    // ========== 测试 4: 软删除
    assert!(dal.delete(new_ctx("admin"), &updated).is_ok());
    let found_after_delete = dal.find_by_id(ctx.clone(), &updated.id()).unwrap();
    assert!(found_after_delete.is_none());

    // ========== 测试 5: find_all 排除已删除
    // 现有两个未删除，一个已删除
    let result = dal.find_all(ctx.clone()).unwrap();
    assert_eq!(result.len(), 2);
    let names: Vec<String> = result.iter().map(|a| a.name().to_string()).collect();
    assert!(names.contains(&"Agent0".to_string()));
    assert!(names.contains(&"Agent1".to_string()));

    // ========== 测试 6: 查询不存在
    let found_none = dal.find_by_id(ctx.clone(), "not-exist-id").unwrap();
    assert!(found_none.is_none());
}
