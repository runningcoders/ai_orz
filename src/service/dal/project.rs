//! Project DAL 模块
//!
//! 职责：Project 领域的数据访问层，封装 ProjectDao 提供统一的查询接口

use common::error::Result;
use common::models::{ModelCallStats, ProjectStats, StatsFetchOptions};
use crate::models::project::Project;
use crate::pkg::RequestContext;
use crate::pkg::stats::ModelCallEvent;
use crate::service::dao::project;
use crate::service::dao::project::{ProjectDao, ProjectQuery, ProjectStatsDao, ProjectStatsQuery};
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use common::enums::ProjectStatus;
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;

// ==================== 单例管理 ====================

static PROJECT_DAL: OnceLock<Arc<dyn ProjectDal + Send + Sync>> = OnceLock::new();

/// 获取 Project DAL 单例
pub fn dal() -> Arc<dyn ProjectDal + Send + Sync> {
    PROJECT_DAL.get().cloned().unwrap()
}

/// 初始化 Project DAL
pub fn init() {
    project::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = PROJECT_DAL.set(new(
        project::dao(),
        project::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
    ));
}

/// 创建 Project DAL（返回 trait 对象）
pub fn new(
    project_dao: Arc<dyn ProjectDao + Send + Sync>,
    project_stats_dao: Arc<dyn ProjectStatsDao<ModelCallEvent = ModelCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
) -> Arc<dyn ProjectDal + Send + Sync> {
    Arc::new(ProjectDalImpl { project_dao, project_stats_dao, model_provider_stats_dao })
}

// ==================== DAL 接口 ====================

/// Project DAL 接口
#[async_trait::async_trait]
pub trait ProjectDal: Send + Sync {
    /// 创建项目
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<()>;

    /// 根据 ID 获取项目
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>>;

    /// 获取根用户下的所有项目
    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 获取根用户下指定状态的项目
    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>>;

    /// 通用综合查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<Project>>;

    /// 更新项目信息
    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<()>;

    /// 更新项目状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()>;

    /// 归档项目（软删除）
    async fn archive(
        &self,
        ctx: RequestContext,
        id: &str,
        modified_by: &str,
    ) -> Result<()>;

    /// 统计根用户的项目总数
    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64>;

    /// 统计根用户指定状态的项目数
    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64>;

    // ==================== 统计查询 ====================

    /// 获取 Project 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(&self, ctx: RequestContext, project_id: &str, options: StatsFetchOptions) -> Result<ProjectStats>;

    /// 获取 Project 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 project_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(&self, ctx: RequestContext, project_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}

// ==================== DAL 实现 ====================

/// Project DAL 实现
struct ProjectDalImpl {
    project_dao: Arc<dyn ProjectDao + Send + Sync>,
    project_stats_dao: Arc<dyn ProjectStatsDao<ModelCallEvent = ModelCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
}

#[async_trait::async_trait]
impl ProjectDal for ProjectDalImpl {
    async fn create(&self, ctx: RequestContext, project: &Project) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, project);
        self.project_dao.insert(ctx, &project.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>> {
        let opt = self.project_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Project::from_po))
    }

    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        let list = self
            .project_dao
            .list_by_root_user(ctx, root_user_id, limit)
            .await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Project>> {
        let list = self
            .project_dao
            .list_by_root_user_and_status(ctx, root_user_id, status, limit)
            .await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<Vec<Project>> {
        let list = self.project_dao.query(ctx, query).await?;
        Ok(list.into_iter().map(Project::from_po).collect())
    }

    async fn update(&self, ctx: RequestContext, project: &Project) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, project);
        self.project_dao.update(ctx, &project.po).await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().project_id(id).build();
        self.project_dao
            .update_status(ctx, id, status, modified_by)
            .await
    }

    async fn archive(
        &self,
        ctx: RequestContext,
        id: &str,
        modified_by: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().project_id(id).build();
        self.project_dao
            .update_status(ctx, id, ProjectStatus::Archived, modified_by)
            .await
    }

    async fn count_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
    ) -> Result<u64> {
        self.project_dao.count_by_root_user(ctx, root_user_id).await
    }

    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64> {
        self.project_dao
            .count_by_root_user_and_status(ctx, root_user_id, status)
            .await
    }

    // ==================== 统计查询 ====================

    async fn get_stats(&self, ctx: RequestContext, project_id: &str, options: StatsFetchOptions) -> Result<ProjectStats> {
        let query = ProjectStatsQuery {
            project_id: project_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.project_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(&self, ctx: RequestContext, project_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            project_id: Some(project_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
    }
}
