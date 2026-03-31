//! Agent Domain 模块
//!
//! Domain 层是抽象业务层，关注 Agent 的通用行为逻辑
//! 组合多个 DAL 完成业务逻辑，不关心具体的 Agent 类型
//! 这一层的方法更凝练，行为的核心是调度逻辑

use crate::error::AppError;
use crate::pkg::RequestContext;
use crate::service::dal::agent::{Agent, AgentDalTrait};
use std::sync::Arc;

/// Agent Domain 业务逻辑
pub struct AgentDomain {
    agent_dal: Arc<dyn AgentDalTrait>,
}

impl AgentDomain {
    /// 创建 Domain 实例
    pub fn new(agent_dal: Arc<dyn AgentDalTrait>) -> Self {
        Self { agent_dal }
    }

    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    pub fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.create(ctx, agent)
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    pub fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dal.find_by_id(ctx, id)
    }

    /// 列出所有 Agent
    ///
    /// 基础操作：查询所有有效的 Agent
    pub fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.find_all(ctx)
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    pub fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.update(ctx, agent)
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    pub fn delete_agent(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.agent_dal.delete(ctx, id)
    }
}

// ==================== 单例管理 ====================

use std::sync::OnceLock;

static AGENT_DOMAIN: OnceLock<Arc<AgentDomain>> = OnceLock::new();

/// 获取 Agent Domain 单例
pub fn domain() -> Arc<AgentDomain> {
    AGENT_DOMAIN.get().cloned().unwrap()
}

/// 初始化 Agent Domain
pub fn init(agent_dal: Arc<dyn AgentDalTrait>) {
    let _ = AGENT_DOMAIN.set(Arc::new(AgentDomain::new(agent_dal)));
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentPo;
    use crate::pkg::storage;

    fn new_ctx(user_id: &str) -> RequestContext {
        RequestContext::new(Some(user_id.to_string()), None)
    }

    fn setup_test_dal() -> Arc<dyn AgentDalTrait> {
        // 初始化内存数据库
        let conn = storage::test_conn();
        conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
            .unwrap();

        // 创建测试用的 DAO 实现
        struct TestAgentDao {
            conn: std::sync::Mutex<rusqlite::Connection>,
        }

        impl crate::service::dao::agent::AgentDaoTrait for TestAgentDao {
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

            fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
                let conn = self.conn.lock().unwrap();
                conn.execute(
                    "UPDATE agents SET status = 0, modified_by = ?1, updated_at = ?2 WHERE id = ?3 AND status != 0",
                    rusqlite::params![ctx.uid(), current_timestamp(), id],
                )
                .map_err(|e| AppError::Internal(e.to_string()))?;
                Ok(())
            }
        }

        // 创建 DAL
        let dao = Arc::new(TestAgentDao {
            conn: std::sync::Mutex::new(conn),
        });
        Arc::new(crate::service::dal::agent::AgentDal::new(dao))
    }

    fn current_timestamp() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn test_create_agent() {
        let dal = setup_test_dal();
        let domain = AgentDomain::new(dal);
        let ctx = new_ctx("admin");

        let agent_po = AgentPo::new(
            "TestAgent".to_string(),
            "worker".to_string(),
            vec!["coding".to_string()],
            "A helpful agent".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);

        domain.create_agent(ctx, &agent).unwrap();
    }

    #[test]
    fn test_get_agent() {
        let dal = setup_test_dal();
        let domain = AgentDomain::new(dal);
        let ctx = new_ctx("admin");

        let agent_po = AgentPo::new(
            "TestAgent".to_string(),
            "worker".to_string(),
            vec!["coding".to_string()],
            "A helpful agent".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po.clone());
        domain.create_agent(ctx.clone(), &agent).unwrap();

        let found = domain.get_agent(ctx, &agent_po.id).unwrap().unwrap();
        assert_eq!(found.name(), "TestAgent");
    }

    #[test]
    fn test_list_agents() {
        let dal = setup_test_dal();
        let domain = AgentDomain::new(dal);
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
            domain.create_agent(ctx.clone(), &agent).unwrap();
        }

        let agents = domain.list_agents(ctx).unwrap();
        assert_eq!(agents.len(), 3);
    }

    #[test]
    fn test_update_agent() {
        let dal = setup_test_dal();
        let domain = AgentDomain::new(dal);
        let ctx = new_ctx("admin");

        let agent_po = AgentPo::new(
            "Original".to_string(),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain.create_agent(ctx.clone(), &agent).unwrap();

        let mut updated = agent.clone();
        updated.po.name = "Updated".to_string();
        domain.update_agent(new_ctx("editor"), &updated).unwrap();

        let found = domain.get_agent(ctx, &agent.id()).unwrap().unwrap();
        assert_eq!(found.name(), "Updated");
    }

    #[test]
    fn test_delete_agent() {
        let dal = setup_test_dal();
        let domain = AgentDomain::new(dal);
        let ctx = new_ctx("admin");

        let agent_po = AgentPo::new(
            "ToDelete".to_string(),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain.create_agent(ctx.clone(), &agent).unwrap();

        domain.delete_agent(ctx.clone(), &agent.id()).unwrap();
        assert!(domain.get_agent(ctx, &agent.id()).unwrap().is_none());
    }
}
