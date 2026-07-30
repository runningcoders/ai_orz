//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织、超级管理员和默认 ModelProvider
//! Handler 层负责跨 domain 编排：organization domain 创建 org+user，finance domain 创建 provider
//!
//! 初始化是异步任务：提交后返回 task_id，前端轮询进度接口获取执行状态。
//! 任务对象自包含进度状态，通过通用后台任务模块（pkg::background_task）注册运行。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{registry, BackgroundTask};
use crate::service::domain::system;
use crate::service::domain::{finance, organization};
use async_trait::async_trait;
use ai_orz_macros::generate_http_handler;
use common::api::{
    CheckInitializedRequest, GetInitProgressRequest, InitProgressResponse, InitStatus,
    InitializeSystemRequest, InitializeSystemResponse, TaskIdResponse, TaskProgressSnapshot,
    TaskStatus, TaskType,
};
use common::error::{Error, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 系统初始化后台任务（自包含进度状态）
///
/// 持有 ctx/params + 进度状态字段，`run` 方法内部更新进度，
/// 外部通过 `progress()` 读取快照。注册到通用后台任务注册中心后由 registry spawn 执行。
pub struct InitializeSystemTask {
    task_id: String,
    ctx: RequestContext,
    params: InitializeSystemRequest,
    // 进度状态字段（任务体内部更新，外部通过 progress() 读取）
    status: Mutex<TaskStatus>,
    current_step: AtomicUsize,
    total_steps: usize,
    step_message: Mutex<String>,
    started_at: i64,
    finished_at: Mutex<Option<i64>>,
    error: Mutex<Option<String>>,
    result: Mutex<Option<serde_json::Value>>,
}

impl InitializeSystemTask {
    /// 创建新的初始化任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: InitializeSystemRequest) -> Self {
        let total_steps = if params.embedding_model.is_some() {
            5
        } else {
            4
        };
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            ctx,
            params,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            total_steps,
            step_message: Mutex::new("等待开始".to_string()),
            started_at: chrono::Utc::now().timestamp_millis(),
            finished_at: Mutex::new(None),
            error: Mutex::new(None),
            result: Mutex::new(None),
        }
    }

    /// 更新当前步骤进度（任务体内部调用）
    fn set_step(&self, step: usize, message: &str) {
        self.current_step.store(step, Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message.to_string();
    }

    /// 标记完成并写入结果
    fn set_completed(&self, result: serde_json::Value) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for InitializeSystemTask {
    fn task_id(&self) -> &str {
        &self.task_id
    }

    fn task_type(&self) -> TaskType {
        TaskType::InitializeSystem
    }

    fn progress(&self) -> TaskProgressSnapshot {
        TaskProgressSnapshot {
            task_id: self.task_id.clone(),
            task_type: self.task_type().as_str().to_string(),
            status: *self.status.lock().unwrap(),
            current_step: self.current_step.load(Ordering::SeqCst),
            total_steps: self.total_steps,
            step_message: self.step_message.lock().unwrap().clone(),
            started_at: self.started_at,
            finished_at: *self.finished_at.lock().unwrap(),
            error: self.error.lock().unwrap().clone(),
            result: self.result.lock().unwrap().clone(),
        }
    }

    async fn run(&self) -> Result<serde_json::Value> {
        *self.status.lock().unwrap() = TaskStatus::Running;
        self.set_step(1, "正在创建组织和超级管理员");

        match self.run_steps().await {
            Ok(resp) => {
                let v = serde_json::to_value(&resp)
                    .map_err(|e| Error::internal(format!("序列化结果失败: {}", e)))?;
                self.set_completed(v.clone());
                Ok(v)
            }
            Err(e) => {
                self.set_failed(e.to_string());
                Err(e)
            }
        }
    }
}

impl InitializeSystemTask {
    /// 执行初始化步骤（从原 run_initialize_steps 迁移逻辑）
    ///
    /// 每步通过 `set_step` 更新进度，保持原有业务逻辑不变：
    /// 创建组织+Owner → chat provider → embedding provider（可选）→ 同步内置工具 → 导入预置技能
    async fn run_steps(&self) -> Result<InitializeSystemResponse> {
        let ctx = self.ctx.clone();
        let params = self.params.clone();

        // Step 1: 创建组织 + Owner
        self.set_step(1, "正在创建组织和超级管理员");
        let (org_id, user_id) = organization::domain()
            .organization_manage()
            .create_org_and_owner(ctx.clone(), params.clone())
            .await?;

        // Step 2: 创建 chat provider
        self.set_step(2, "正在配置对话模型");
        let chat_provider = crate::models::model_provider::ModelProvider::new(
            params.chat_model.name,
            common::enums::ProviderType::from_i32(params.chat_model.provider_type),
            common::enums::ModelCapability::Agent,
            params.chat_model.model_name,
            params.chat_model.api_key,
            params.chat_model.base_url,
            params.chat_model.description,
            user_id.clone(),
        );
        let chat_provider_id = chat_provider.po.id.clone();
        finance::domain()
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &chat_provider)
            .await?;

        // Step 3: 创建 embedding provider（可选）
        let mut next_step = 3;
        let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
            self.set_step(3, "正在配置向量模型");
            let embedding_provider = crate::models::model_provider::ModelProvider::new(
                embedding_config.name,
                common::enums::ProviderType::from_i32(embedding_config.provider_type),
                common::enums::ModelCapability::Embedding,
                embedding_config.model_name,
                embedding_config.api_key,
                embedding_config.base_url,
                embedding_config.description,
                user_id.clone(),
            );
            let provider_id = embedding_provider.po.id.clone();
            finance::domain()
                .model_provider_manage()
                .create_model_provider(ctx.clone(), &embedding_provider)
                .await?;
            next_step = 4;
            Some(provider_id)
        } else {
            None
        };

        // Step 4/5: 同步内置工具到 DB
        self.set_step(next_step, "正在同步内置工具");
        let tool_count = finance::domain()
            .tool_provider_manage()
            .sync_builtin_tools(ctx.clone())
            .await?;
        sys_info!("initialize_system: 同步 {} 个内置工具到 DB", tool_count);

        // Step 5/6: 导入预置技能
        self.set_step(next_step + 1, "正在导入预置技能");
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        let skill_result = crate::handlers::system::seed::apply_preset_skills(
            ctx.clone(),
            &snapshot.skills,
            Some(&user_id),
            false,
        )
        .await?;
        sys_info!(
            "initialize_system: 导入预置技能（created={}, updated={}, skipped={}, author={}）",
            skill_result.created,
            skill_result.updated,
            skill_result.skipped,
            user_id
        );

        Ok(InitializeSystemResponse {
            organization_id: org_id,
            user_id,
            chat_provider_id,
            embedding_provider_id,
        })
    }
}

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<bool> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    Ok(initialized)
}

/// 初始化系统（异步提交，返回 task_id）
///
/// 创建自包含任务对象，注册到通用后台任务注册中心，由 registry spawn 执行。
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<TaskIdResponse> {
    let task = Arc::new(InitializeSystemTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}

/// 查询初始化进度
///
/// 从通用后台任务注册中心获取基础进度快照，装饰为 `InitProgressResponse` 返回：
/// - 将 `TaskStatus` 映射为 `InitStatus`
/// - 解析 `result` JSON 字段为 `InitializeSystemResponse`
#[generate_http_handler]
pub async fn get_initialize_progress(
    _ctx: RequestContext,
    params: GetInitProgressRequest,
) -> Result<InitProgressResponse> {
    let snapshot = system::domain()
        .background_task_registry()
        .get_progress(&params.task_id)
        .await
        .ok_or_else(|| Error::not_found(format!("初始化任务不存在: {}", params.task_id)))?;

    Ok(InitProgressResponse {
        task_id: snapshot.task_id,
        status: match snapshot.status {
            TaskStatus::Pending => InitStatus::Pending,
            TaskStatus::Running => InitStatus::Running,
            TaskStatus::Completed => InitStatus::Completed,
            TaskStatus::Failed => InitStatus::Failed,
        },
        current_step: snapshot.current_step,
        total_steps: snapshot.total_steps,
        step_message: snapshot.step_message,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        error: snapshot.error,
        result: snapshot
            .result
            .and_then(|v| serde_json::from_value(v).ok()),
    })
}
