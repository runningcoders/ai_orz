//! Artifact DAL 模块
//!
//! 职责：Artifact 领域的数据访问层，封装 ArtifactDao 提供统一的查询接口

use common::error::Result;
use crate::models::artifact::Artifact;
use crate::pkg::RequestContext;
use crate::service::dao::artifact;
use crate::service::dao::artifact::ArtifactDao;
use common::enums::{ArtifactSourceType, FileType};
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;

/// Artifact DAL 查询参数。
///
/// Domain 层只依赖 DAL 的查询对象，不暴露 DAO 查询结构。
#[derive(Debug, Clone, Default)]
pub struct ArtifactQuery {
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub file_type: Option<FileType>,
    pub source_type: Option<ArtifactSourceType>,
    pub pagination: common::api::PaginationParams,
}

// ==================== 单例管理 ====================

static ARTIFACT_DAL: OnceLock<Arc<dyn ArtifactDal + Send + Sync>> = OnceLock::new();

/// 获取 Artifact DAL 单例
pub fn dal() -> Arc<dyn ArtifactDal + Send + Sync> {
    ARTIFACT_DAL.get().cloned().unwrap()
}

/// 初始化 Artifact DAL
pub fn init() {
    let _ = ARTIFACT_DAL.set(new(artifact::dao()));
}

/// 创建 Artifact DAL（返回 trait 对象）
pub fn new(artifact_dao: Arc<dyn ArtifactDao + Send + Sync>) -> Arc<dyn ArtifactDal + Send + Sync> {
    Arc::new(ArtifactDalImpl { artifact_dao })
}

// ==================== DAL 接口 ====================

/// Artifact DAL 接口
#[async_trait::async_trait]
pub trait ArtifactDal: Send + Sync {
    /// 创建产物
    async fn create(&self, ctx: RequestContext, artifact: &Artifact) -> Result<()>;

    /// 根据 ID 获取产物
    async fn find_by_id(&self, ctx: RequestContext, id: &str)
    -> Result<Option<Artifact>>;

    /// 获取项目下的所有产物
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Artifact>>;

    /// 获取任务下的所有产物
    async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ArtifactQuery,
    ) -> Result<common::api::PagedResult<Artifact>>;

    /// 更新产物状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: i32,
    ) -> Result<()>;

    /// 删除产物（软删除）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 统计项目下的产物数量
    async fn count_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<u64>;

    /// 统计任务下的产物数量
    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// Update an existing artifact full record.
    async fn update(&self, ctx: RequestContext, artifact: &Artifact) -> Result<()>;

    /// Read artifact content (for generated content artifacts).
    async fn read_content(
        &self,
        ctx: RequestContext,
        artifact: &Artifact,
    ) -> Result<Option<Vec<u8>>>;

    /// Write artifact content (for generated content artifacts).
    async fn write_content(
        &self,
        ctx: RequestContext,
        artifact: &Artifact,
        content: &[u8],
    ) -> Result<()>;
}

// ==================== DAL 实现 ====================

/// Artifact DAL 实现
struct ArtifactDalImpl {
    artifact_dao: Arc<dyn ArtifactDao + Send + Sync>,
}

#[async_trait::async_trait]
impl ArtifactDal for ArtifactDalImpl {
    async fn create(&self, ctx: RequestContext, artifact: &Artifact) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, artifact);
        self.artifact_dao.insert(ctx, &artifact.po).await
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Artifact>> {
        let opt = self.artifact_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Artifact::from_po))
    }

    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Artifact>> {
        let po_list = self.artifact_dao.list_by_project(ctx, project_id).await?;
        Ok(po_list.into_iter().map(Artifact::from_po).collect())
    }

    async fn list_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Artifact>> {
        let po_list = self.artifact_dao.list_by_task(ctx, task_id).await?;
        Ok(po_list.into_iter().map(Artifact::from_po).collect())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ArtifactQuery,
    ) -> Result<common::api::PagedResult<Artifact>> {
        let page = self
            .artifact_dao
            .query(
                ctx,
                artifact::ArtifactQuery {
                    project_id: query.project_id,
                    task_id: query.task_id,
                    file_type: query.file_type,
                    source_type: query.source_type,
                    pagination: query.pagination,
                },
            )
            .await?;
        Ok(page.map(Artifact::from_po))
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: i32,
    ) -> Result<()> {
        self.artifact_dao.update_status(ctx, id, status).await
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.artifact_dao.delete(ctx, id).await
    }

    async fn count_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<u64> {
        self.artifact_dao
            .count_by_project(ctx, project_id)
            .await
            .map(|v| v as u64)
    }

    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<u64> {
        self.artifact_dao
            .count_by_task(ctx, task_id)
            .await
            .map(|v| v as u64)
    }

    async fn update(&self, ctx: RequestContext, artifact: &Artifact) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, artifact);
        self.artifact_dao.update(ctx, &artifact.po).await
    }

    async fn read_content(
        &self,
        ctx: RequestContext,
        artifact: &Artifact,
    ) -> Result<Option<Vec<u8>>> {
        self.artifact_dao.read_content(ctx, &artifact.po).await
    }

    async fn write_content(
        &self,
        ctx: RequestContext,
        artifact: &Artifact,
        content: &[u8],
    ) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, artifact);
        self.artifact_dao
            .write_content(ctx, &artifact.po, content)
            .await
    }
}