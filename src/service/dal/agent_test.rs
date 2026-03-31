//! Agent DAL 单元测试

use super::*;
use crate::models::agent::AgentPo;
use crate::pkg::storage;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_dao() -> Arc<dyn AgentDaoTrait> {
    // 初始化内存数据库
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();

    // 创建测试用的 DAO 实现
    struct TestAgentDao {
        conn: std::sync::Mutex<rusqlite::Connection>,
    }

    impl AgentDaoTrait for TestAgentDao {
        fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
            let conn = self.conn.lock().unwrap();
            let now = current_timestamp();
            conn.execute(
                "INSERT INTO agents (id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    agent.id,
                    agent.name,
                    agent.role,
                    agent.capabilities,
                    agent.soul,
                    agent.status.to_i32(),
                    ctx.uid(),
                    ctx.uid(),
                    now,
                    now,
                ],
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(())
        }

        fn find_by_id(
            &self,
            _ctx: RequestContext,
            id: &str,
        ) -> Result<Option<AgentPo>, AppError> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at 
                     FROM agents WHERE id = ?1 AND status != 0",
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;

            match stmt.query_row([id], |row| {
                Ok(AgentPo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    role: row.get(2)?,
                    capabilities: row.get(3)?,
                    soul: row.get(4)?,
                    status: crate::pkg::constants::AgentPoStatus::from_i32(row.get(5)?),
                    created_by: row.get(6)?,
                    modified_by: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            }) {
                Ok(a) => Ok(Some(a)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(AppError::Internal(e.to_string())),
            }
        }

        fn find_all(&self, _ctx: RequestContext) -> Result<Vec<AgentPo>, AppError> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, role, capabilities, soul, status, created_by, modified_by, created_at, updated_at 
                     FROM agents WHERE status != 0 ORDER BY id DESC",
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;

            let agents = stmt
                .query_map([], |row| {
                    Ok(AgentPo {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        role: row.get(2)?,
                        capabilities: row.get(3)?,
                        soul: row.get(4)?,
                        status: crate::pkg::constants::AgentPoStatus::from_i32(row.get(5)?),
                        created_by: row.get(6)?,
                        modified_by: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                })
                .map_err(|e| AppError::Internal(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Internal(e.to_string()))?;

            Ok(agents)
        }

        fn update(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE agents SET name = ?1, role = ?2, capabilities = ?3, soul = ?4, modified_by = ?5, updated_at = ?6 WHERE id = ?7",
                rusqlite::params![
                    agent.name,
                    agent.role,
                    agent.capabilities,
                    agent.soul,
                    ctx.uid(),
                    current_timestamp(),
                    agent.id,
                ],
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(())
        }

        fn delete(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError> {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
                rusqlite::params![ctx.uid(), current_timestamp(), agent.id],
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(())
        }
    }

    // 创建 DAL
    Arc::new(TestAgentDao {
        conn: std::sync::Mutex::new(conn),
    })
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn test_create_and_find_by_id() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "A helpful agent".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    dal.create(ctx.clone(), &agent).unwrap();
    let found = dal.find_by_id(ctx, &agent.id()).unwrap().unwrap();

    assert_eq!(found.name(), "TestAgent");
    assert_eq!(found.po.created_by, "admin");
}

#[test]
fn test_find_all() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("admin");

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        dal.create(ctx.clone(), &agent).unwrap();
    }

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_update() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    dal.create(ctx.clone(), &agent).unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    dal.update(new_ctx("editor"), &updated).unwrap();

    let found = dal.find_by_id(ctx, &updated.id()).unwrap().unwrap();
    assert_eq!(found.name(), "Updated");
    assert_eq!(found.po.modified_by, "editor");
}

#[test]
fn test_delete() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    dal.create(ctx.clone(), &agent).unwrap();

    dal.delete(ctx.clone(), &agent).unwrap();
    assert!(dal.find_by_id(ctx, &agent.id()).unwrap().is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("admin");

    let agent1_po = AgentPo::new("Normal".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    let agent2_po = AgentPo::new("Deleted".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());

    let agent1 = Agent::from_po(agent1_po);
    let agent2 = Agent::from_po(agent2_po);

    dal.create(ctx.clone(), &agent1).unwrap();
    dal.create(ctx.clone(), &agent2).unwrap();
    dal.delete(ctx.clone(), &agent2).unwrap();

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name(), "Normal");
}

#[test]
fn test_find_not_exists() {
    let dao = setup_test_dao();
    let dal = Arc::new(AgentDal::new(dao));
    let ctx = new_ctx("user1");

    assert!(dal.find_by_id(ctx, "not-exists").unwrap().is_none());
}
