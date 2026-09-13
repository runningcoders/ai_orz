//! Task Vector DAO implementation
//! 负责任务向量索引的 CRUD 操作，与基础任务数据完全解耦

use crate::models::vector::{
    FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorRow, VectorSearchHit,
};
use crate::pkg::RequestContext;
use crate::service::dao::task::{TaskQuery, TaskVectorDao};
use async_trait::async_trait;
use common::enums::AssigneeType;
use common::error::{Result, err};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static TASK_VECTOR_DAO: OnceLock<Arc<dyn TaskVectorDao>> = OnceLock::new();

/// 创建一个全新的 Task Vector DAO 实例（用于测试）
pub fn new() -> Arc<dyn TaskVectorDao> {
    Arc::new(TaskVectorDaoImpl)
}

/// 获取 Task Vector DAO 单例
pub fn dao() -> Arc<dyn TaskVectorDao> {
    TASK_VECTOR_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = TASK_VECTOR_DAO.set(new());
}

// ==================== 实现 ====================

/// 任务向量 DAO 实现
/// 基于存储层通用 VectorStore trait，不绑定具体数据库
#[derive(Debug, Clone)]
pub struct TaskVectorDaoImpl;

#[async_trait]
impl TaskVectorDao for TaskVectorDaoImpl {
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        task_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()> {
        let vector_store = ctx.vector_store();
        vector_store.upsert("tasks", task_id, vector_params).await?;
        Ok(())
    }

    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &TaskQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("tasks", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }

    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Option<VectorRow>> {
        ctx.vector_store()
            .get("tasks", task_id)
            .await
            .map_err(|e| err!(Internal, "Vector store error: {e}").with_source(e))
    }

    async fn delete_vector(&self, ctx: RequestContext, task_id: &str) -> Result<()> {
        let vector_store = ctx.vector_store();
        vector_store.delete("tasks", task_id).await?;
        Ok(())
    }

    async fn clear_collection(&self, _ctx: RequestContext) -> Result<()> {
        _ctx.vector_store().clear_collection("tasks").await?;
        Ok(())
    }
}

// ==================== TaskQuery → VectorFilter 白名单转译 ====================

/// TaskQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：project_id / assignee（assignee_type=Agent 时，与 payload.agent_id 同构）。
/// 未覆盖字段（status_in / ids / keyword / pagination）由回业务表过滤兜底；
/// payload 未落 status，故 status_in 不下推。
pub(crate) fn translate_filters(query: &TaskQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(project_id) = &query.project_id {
        conditions.push(VectorFilter::Eq(
            VectorField::ProjectId,
            FilterValue::Str(project_id.clone()),
        ));
    }
    // payload.agent_id 仅在 assignee_type=Agent 时落库（TaskPo::vector_payload 同构），
    // 非 Agent 指派（如 User）或未指定类型时不满足谓词语义，不下推
    if query.assignee_type == Some(AssigneeType::Agent)
        && let Some(assignee_id) = &query.assignee_id
    {
        conditions.push(VectorFilter::Eq(
            VectorField::AgentId,
            FilterValue::Str(assignee_id.clone()),
        ));
    }

    match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(VectorFilter::All(conditions)),
    }
}

#[cfg(test)]
mod translate_tests {
    use super::*;

    #[test]
    fn test_translate_project_only() {
        let q = TaskQuery {
            project_id: Some("proj-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::ProjectId, FilterValue::Str("proj-1".into()))
        );
    }

    #[test]
    fn test_translate_agent_assignee() {
        let q = TaskQuery {
            assignee_type: Some(AssigneeType::Agent),
            assignee_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()))
        );
    }

    #[test]
    fn test_translate_non_agent_assignee_not_pushed() {
        // assignee_type 非 Agent：payload.agent_id 未落库，不能下推
        let q = TaskQuery {
            assignee_type: Some(AssigneeType::User),
            assignee_id: Some("user-1".into()),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());

        // assignee_id 单独设置但类型未指定：语义不明，保守不下推
        let q = TaskQuery {
            assignee_id: Some("user-1".into()),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }

    #[test]
    fn test_translate_combined_all() {
        let q = TaskQuery {
            project_id: Some("proj-1".into()),
            assignee_type: Some(AssigneeType::Agent),
            assignee_id: Some("agent-1".into()),
            ..Default::default()
        };
        let f = translate_filters(&q).unwrap();
        match f {
            VectorFilter::All(fs) => assert_eq!(fs.len(), 2),
            other => panic!("expect All, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&TaskQuery::default()).is_none());
        // status_in：payload 未落 status，不下推
        let q = TaskQuery {
            status_in: Some(vec![common::enums::TaskStatus::InProgress]),
            ..Default::default()
        };
        assert!(translate_filters(&q).is_none());
    }
}
