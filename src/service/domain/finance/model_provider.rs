//! Model Provider 具体实现

use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::domain::finance::{FinanceDomainImpl, ModelProviderManage, RebuildTask};
use common::enums::ModelProviderStatus;
use common::error::{Error, ErrorCode, ErrorField, Result};
use serde_json::json;
use std::sync::Arc;

use crate::enrich_ctx;

#[async_trait::async_trait]
impl ModelProviderManage for FinanceDomainImpl {
    async fn create_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.create(ctx, provider).await
    }

    async fn get_model_provider(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProvider>> {
        self.model_provider_dal.find_by_id(ctx, id).await
    }

    async fn get_model_provider_with_options(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::model_provider::ModelProviderFetchOptions,
    ) -> Result<Option<ModelProvider>> {
        self.model_provider_dal
            .get_model_provider(ctx, id, options)
            .await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::model_provider::ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProvider>> {
        self.model_provider_dal.query(ctx, query).await
    }

    async fn list_model_providers(&self, ctx: RequestContext) -> Result<Vec<ModelProvider>> {
        self.model_provider_dal.find_all(ctx).await
    }

    async fn update_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        if provider.po.capability.is_embedding()
            && provider.po.status == ModelProviderStatus::Normal
        {
            if let Some(current) = self
                .model_provider_dal
                .find_enabled_embedding_provider(ctx.clone())
                .await?
            {
                if current.po.id != provider.po.id {
                    let mut field = ErrorField::new();
                    field.insert("current_provider_id".into(), json!(current.po.id));
                    field.insert("current_provider_name".into(), json!(current.po.name));
                    return Err(Error::new(
                        ErrorCode::EmbeddingProviderSwitchRequired,
                        format!(
                            "Another embedding provider '{}' is already enabled",
                            current.po.name
                        ),
                    )
                    .with_field(field));
                }
            }
        }

        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.update(ctx, provider).await
    }

    async fn delete_model_provider(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
    ) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.model_provider_dal.delete(ctx, provider).await
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        provider: &ModelProvider,
        prompt: &str,
    ) -> Result<String> {
        let ctx = enrich_ctx!(&ctx, provider);
        self.brain_dal.test_connection(ctx, provider, prompt).await
    }

    async fn switch_embedding_provider(
        &self,
        ctx: RequestContext,
        new_provider_id: &str,
    ) -> Result<(Option<ModelProvider>, String)> {
        let new_provider = self
            .get_model_provider(ctx.clone(), new_provider_id)
            .await?
            .ok_or_else(|| {
                Error::not_found(format!("ModelProvider {} not found", new_provider_id))
            })?;

        if !new_provider.po.capability.is_embedding() {
            return Err(Error::bad_request(
                "Target provider is not an embedding provider",
            ));
        }

        let current_provider = self
            .model_provider_dal
            .find_enabled_embedding_provider(ctx.clone())
            .await?;

        if let Some(ref current) = current_provider {
            if current.po.id == new_provider_id {
                // 同一 provider，无需重建
                return Ok((current_provider, String::new()));
            }
        }

        if let Some(mut current) = current_provider.clone() {
            current.po.status = ModelProviderStatus::Deleted;
            self.model_provider_dal
                .update(ctx.clone(), &current)
                .await?;
        }

        let mut new_provider_to_enable = new_provider.clone();
        new_provider_to_enable.po.status = ModelProviderStatus::Normal;
        self.update_model_provider(ctx.clone(), &new_provider_to_enable)
            .await?;

        // 异步启动向量索引重建
        let task_id = self.start_rebuild_task(ctx).await?;

        Ok((current_provider, task_id))
    }

    async fn get_rebuild_progress(
        &self,
        _ctx: RequestContext,
        task_id: &str,
    ) -> Result<Option<common::api::RebuildProgressResponse>> {
        let guard = self.rebuild_task.read().await;
        if let Some(task) = guard.as_ref() {
            if task.task_id == task_id {
                return Ok(Some(common::api::RebuildProgressResponse {
                    task_id: task.task_id.clone(),
                    status: task.status.clone(),
                    current_entity: task.current_entity.clone(),
                    current_entity_index: task.current_entity_index,
                    total_entities: task.total_entities,
                    processed_records: task.processed_records,
                    total_records: task.total_records,
                    started_at: task.started_at,
                    finished_at: task.finished_at,
                    error: task.error.clone(),
                }));
            }
        }
        Ok(None)
    }
}

impl FinanceDomainImpl {
    /// 启动后台向量索引重建任务
    ///
    /// 如果已有任务运行中，返回 `RebuildInProgress` 错误（附带 task_id 字段）。
    pub async fn start_rebuild_task(&self, ctx: RequestContext) -> Result<String> {
        // 检查是否已有任务运行
        {
            let guard = self.rebuild_task.read().await;
            if let Some(task) = guard.as_ref() {
                if task.status == common::api::RebuildStatus::Running
                    || task.status == common::api::RebuildStatus::Pending
                {
                    let mut field = ErrorField::new();
                    field.insert("task_id".into(), json!(task.task_id));
                    return Err(Error::new(
                        ErrorCode::RebuildInProgress,
                        "A rebuild task is already in progress",
                    )
                    .with_field(field));
                }
            }
        }

        let task_id = uuid::Uuid::new_v4().to_string();
        let started_at = chrono::Utc::now().timestamp_millis();
        let total_entities: usize = 7;

        // 克隆后台任务所需引用
        let rebuild_task_ref = self.rebuild_task.clone();
        let ctx_clone = ctx.clone();
        let task_id_clone = task_id.clone();

        // spawn 后台重建任务
        let handle = tokio::spawn(async move {
            Self::run_rebuild_task(rebuild_task_ref, task_id_clone, ctx_clone, started_at).await;
        });

        // 存储 RebuildTask
        {
            let mut guard = self.rebuild_task.write().await;
            *guard = Some(RebuildTask {
                task_id: task_id.clone(),
                status: common::api::RebuildStatus::Running,
                current_entity: None,
                current_entity_index: 0,
                total_entities,
                processed_records: 0,
                total_records: 0,
                started_at,
                finished_at: None,
                error: None,
                task_handle: handle,
            });
        }

        log_info!(
            &ctx,
            "rebuild_vectors",
            "已启动异步向量索引重建任务 task_id={}",
            task_id
        );

        Ok(task_id)
    }

    /// 执行向量索引重建（后台任务体）
    ///
    /// 依次重建 7 个实体的向量索引，单个实体失败只记日志不中断整体流程。
    async fn run_rebuild_task(
        rebuild_task_ref: Arc<tokio::sync::RwLock<Option<RebuildTask>>>,
        task_id: String,
        ctx: RequestContext,
        started_at: i64,
    ) {
        use crate::service::dal;

        log_info!(
            &ctx,
            "rebuild_vectors",
            "开始异步重建所有向量索引 task_id={}",
            task_id
        );

        let entities: [&str; 7] = [
            "agent", "memory", "skill", "task", "project", "message", "tool",
        ];
        let total = entities.len();

        for (i, name) in entities.iter().enumerate() {
            // 重建前更新进度
            {
                let mut guard = rebuild_task_ref.write().await;
                if let Some(task) = guard.as_mut() {
                    task.current_entity = Some((*name).to_string());
                    task.current_entity_index = i;
                }
            }

            let result = match *name {
                "agent" => dal::agent::dal().rebuild_vectors(ctx.clone()).await,
                "memory" => dal::memory::dal().rebuild_vectors(ctx.clone()).await,
                "skill" => dal::skill::dal().rebuild_vectors(ctx.clone()).await,
                "task" => dal::task::dal().rebuild_vectors(ctx.clone()).await,
                "project" => dal::project::dal().rebuild_vectors(ctx.clone()).await,
                "message" => dal::message::dal().rebuild_vectors(ctx.clone()).await,
                "tool" => dal::tool::dal().rebuild_vectors(ctx.clone()).await,
                _ => Ok(()),
            };

            if let Err(e) = result {
                log_warn!(
                    &ctx,
                    "rebuild_vectors",
                    error = ?e,
                    "{} 向量重建失败",
                    name
                );
            }
        }

        // 标记完成
        {
            let mut guard = rebuild_task_ref.write().await;
            if let Some(task) = guard.as_mut() {
                task.status = common::api::RebuildStatus::Completed;
                task.current_entity = None;
                task.current_entity_index = total;
                task.finished_at = Some(chrono::Utc::now().timestamp_millis());
            }
        }

        let _ = started_at; // started_at 已在 start_rebuild_task 中记录
        log_info!(
            &ctx,
            "rebuild_vectors",
            "异步重建所有向量索引完成 task_id={}",
            task_id
        );
    }
}
